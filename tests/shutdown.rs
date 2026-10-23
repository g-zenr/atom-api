//! The shutdown path, exercised with a real signal.
//!
//! `src/main.rs` stops accepting as soon as the signal arrives and bounds the
//! drain that follows with `app.limits.shutdown_grace_seconds`. The two tests
//! here pin the two halves of that behaviour:
//!
//! | Test | What it pins |
//! | ---- | ------------ |
//! | [`a_signal_stops_the_process_without_waiting_for_the_grace`] | the future given to `with_graceful_shutdown` resolves immediately, so an idle process exits at once instead of after the grace |
//! | [`the_drain_is_bounded_when_an_in_flight_request_cannot_finish`] | the drain is cut off at the grace, so a request that never completes cannot keep the process alive |
//!
//! Both spawn the real binary against the disposable test database and deliver
//! a real `SIGTERM`, because the defect these guard against cannot be seen from
//! a unit test or from reading the code. The earlier implementation slept
//! *inside* the signal future: the listener kept accepting for the whole grace
//! and the drain that followed had no bound. A test that only asserts "the
//! process exits after the signal" passes against both implementations, so
//! neither test below asserts that. Each one asserts a difference:
//!
//! - an idle process must exit **well before** the grace elapses, which fails
//!   when the signal future sleeps the grace away first;
//! - a process with an in-flight request must exit **at** the grace and no
//!   later, which fails when the drain is unbounded.
//!
//! Unix only. The signal is delivered with `kill -TERM`, and the binary only
//! installs a `SIGTERM` handler under `cfg(unix)`. On Windows this file
//! compiles to nothing and `cargo test` reports it as no tests run rather than
//! as a failure.
//!
//! Without `TEST_DATABASE_URL` both tests skip instead of failing: the server
//! cannot start without a database, and a checkout without one must not produce
//! a false alarm. The database name must still contain `test`, which is the
//! guard [`common::test_database_url`] enforces.

#[cfg(unix)]
pub mod common;

#[cfg(unix)]
mod unix {
    use std::io::{ErrorKind, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::common;

    /// The signing secret the spawned instance runs with.
    ///
    /// Long enough for the start-up rule, and used only against a disposable
    /// database.
    const TEST_SECRET: &str = "shutdown-test-signing-secret-of-32-bytes";

    /// How long a spawned instance may take to answer its readiness probe.
    const READY_TIMEOUT: Duration = Duration::from_secs(10);

    /// How often the readiness probe is retried.
    const READY_INTERVAL: Duration = Duration::from_millis(50);

    /// The pause between the readiness answer and the signal.
    ///
    /// The `SIGTERM` handler is installed on the first poll of the server
    /// future, microseconds after the listener starts to accept. Without the
    /// pause a fast machine could deliver the signal before the handler exists,
    /// and the process would die by default disposition: a false failure that
    /// says nothing about the code under test.
    const SETTLE: Duration = Duration::from_millis(150);

    /// A `SIGTERM` that is stopped by the process, not by the default action.
    const TERMINATED_CLEANLY: Option<i32> = Some(0);

    /// A spawned instance of the real binary.
    ///
    /// The child is killed when the value is dropped, so a failed assertion or
    /// a panic cannot leave a server running on the machine.
    struct Instance {
        child: Child,
        addr: SocketAddr,
    }

    impl Instance {
        /// Starts the binary on an ephemeral port with the given grace.
        fn spawn(grace_seconds: u64, database_url: &str) -> Self {
            // Port 0 in the configuration would be honoured, but the port the
            // socket actually received is never reported, so the test could not
            // find it. Reserving a free port and releasing it immediately is the
            // usual approximation; the window is a few microseconds wide and a
            // collision would show up as a start-up failure, not as a wrong
            // assertion.
            let port = free_port();
            let addr = SocketAddr::from(([127, 0, 0, 1], port));

            let child = Command::new(env!("CARGO_BIN_EXE_atom-api"))
                // The configuration directory is discovered relative to the
                // working directory. Pointing it at the package root makes the
                // child read the committed files, exactly as `cargo run` does.
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .env("APP_ENV", "development")
                .env("APP_BIND_ADDR", addr.to_string())
                .env("APP_SHUTDOWN_GRACE_SECONDS", grace_seconds.to_string())
                // Warnings stay visible, so a start-up failure is readable in
                // the test output. Nothing asserts on a log line.
                .env("APP_LOG_FILTER", "warn")
                .env("APP_JWT_SECRET", TEST_SECRET)
                .env("DATABASE_URL", database_url)
                .env_remove("ATOM_CONFIG_DIR")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap_or_else(|error| {
                    panic!(
                        "could not start {}: {error}",
                        env!("CARGO_BIN_EXE_atom-api")
                    )
                });

            Self { child, addr }
        }

        /// Polls the readiness endpoint until it answers `200`.
        ///
        /// `/health/ready` runs `SELECT 1`, so the answer proves that the
        /// database is connected and the migrations are applied, which is what
        /// "the process is serving" means here.
        fn wait_until_ready(&mut self) {
            let deadline = Instant::now() + READY_TIMEOUT;

            loop {
                if let Some(status) = self.child.try_wait().expect("the child status is readable") {
                    panic!("the server exited before it became ready: {status}");
                }

                if let Some(response) = http_get(self.addr, "/health/ready") {
                    if response.starts_with("HTTP/1.1 200") {
                        return;
                    }
                }

                if Instant::now() >= deadline {
                    panic!(
                        "the server on {} did not become ready within {READY_TIMEOUT:?}; \
                         check the child's output above",
                        self.addr
                    );
                }

                thread::sleep(READY_INTERVAL);
            }
        }

        /// Waits long enough for the signal handler to exist.
        fn settle(&self) {
            thread::sleep(SETTLE);
        }

        /// Delivers `SIGTERM`.
        fn terminate(&self) {
            let status = Command::new("kill")
                .arg("-TERM")
                .arg(self.child.id().to_string())
                .status()
                .expect("`kill` must be available on a unix host");

            assert!(
                status.success(),
                "could not deliver SIGTERM to the server process: {status}"
            );
        }

        /// Returns the exit status if the process ends within `timeout`.
        fn wait_for_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
            let deadline = Instant::now() + timeout;

            loop {
                if let Some(status) = self.child.try_wait().expect("the child status is readable") {
                    return Some(status);
                }

                if Instant::now() >= deadline {
                    return None;
                }

                thread::sleep(Duration::from_millis(25));
            }
        }

        /// True while the process is still running.
        fn is_running(&mut self) -> bool {
            self.child
                .try_wait()
                .expect("the child status is readable")
                .is_none()
        }
    }

    impl Drop for Instance {
        fn drop(&mut self) {
            // `kill` on an already reaped child is an error, and an error here
            // is not interesting: the point is only that nothing is left.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn a_signal_stops_the_process_without_waiting_for_the_grace() {
        /// Deliberately long, so that a process which slept the grace before
        /// draining is clearly distinguishable from one that did not.
        const GRACE_SECONDS: u64 = 8;

        /// Well below the grace. An idle process needs single-digit
        /// milliseconds, so the headroom is two orders of magnitude.
        const BOUND: Duration = Duration::from_secs(4);

        let Some(database_url) =
            database_url_or_skip("a_signal_stops_the_process_without_waiting_for_the_grace")
        else {
            return;
        };

        let mut instance = Instance::spawn(GRACE_SECONDS, &database_url);
        instance.wait_until_ready();
        instance.settle();

        instance.terminate();

        let Some(status) = instance.wait_for_exit(BOUND) else {
            panic!(
                "the process was still running {BOUND:?} after SIGTERM with no request in \
                 flight, while the grace is {GRACE_SECONDS}s. The future given to \
                 `with_graceful_shutdown` is waiting for the grace instead of resolving on the \
                 signal, so the listener keeps accepting for the whole grace."
            );
        };

        assert_eq!(
            status.code(),
            TERMINATED_CLEANLY,
            "the process did not exit successfully, so the signal was not handled and the \
             default disposition ended it instead: {status}"
        );
    }

    #[test]
    fn the_drain_is_bounded_when_an_in_flight_request_cannot_finish() {
        /// Short, so that the whole test stays well under the suite's patience.
        const GRACE_SECONDS: u64 = 2;

        /// Room for the process to tear down after the deadline. The point is
        /// the order of magnitude, not the millisecond.
        const SLACK: Duration = Duration::from_secs(5);

        let Some(database_url) =
            database_url_or_skip("the_drain_is_bounded_when_an_in_flight_request_cannot_finish")
        else {
            return;
        };

        let mut instance = Instance::spawn(GRACE_SECONDS, &database_url);
        instance.wait_until_ready();
        instance.settle();

        // A request that can never complete: the head declares a body, and the
        // body is never sent. The handler is waiting for it, so the request is
        // in flight for as long as the socket stays open.
        //
        // The socket is held for the rest of the test. Closing it would let the
        // request finish and the drain would have nothing left to wait for.
        let (_stalled, interim) = start_stalled_request(instance.addr);

        assert!(
            interim.starts_with("HTTP/1.1 100"),
            "the server did not answer `100 Continue`, so the request is not known to be in \
             flight and this test could not tell a bounded drain from an unbounded one. \
             Answered: {interim:?}"
        );

        let signalled_at = Instant::now();
        instance.terminate();

        // The request is in flight, so a bounded drain is still running: the
        // process must survive the first part of the grace. If it is already
        // gone, the request was not being drained at all and the result below
        // would prove nothing.
        thread::sleep(Duration::from_millis(500));
        assert!(
            instance.is_running(),
            "the process exited within 500ms of the signal while a request was in flight and \
             the grace is {GRACE_SECONDS}s: the in-flight request was dropped rather than \
             drained"
        );

        let Some(status) = instance.wait_for_exit(Duration::from_secs(GRACE_SECONDS) + SLACK)
        else {
            panic!(
                "the process was still running {SLACK:?} after the {GRACE_SECONDS}s grace \
                 elapsed with a request that can never finish, so the drain is unbounded: an \
                 in-flight request keeps the process alive indefinitely"
            );
        };

        assert_eq!(
            status.code(),
            TERMINATED_CLEANLY,
            "the process did not exit successfully after the bounded drain: {status}"
        );

        // The grace is when the process is expected to give up, so that is what
        // ended it. A quarter of the grace is allowed for the difference between
        // the moment the signal was sent and the moment the deadline started, so
        // that the assertion is about behaviour rather than about a millisecond.
        assert!(
            signalled_at.elapsed() >= Duration::from_secs(GRACE_SECONDS) / 4 * 3,
            "the process exited before the deadline of the drain although a request was in \
             flight, so something other than the grace ended it"
        );
    }

    /// Reads the URL of the disposable database, or explains why the test is
    /// skipped.
    ///
    /// The name guard lives in [`common::test_database_url`] and is not
    /// duplicated here: a misconfigured URL must fail loudly, not skip.
    fn database_url_or_skip(test: &str) -> Option<String> {
        let is_set =
            std::env::var(common::TEST_DATABASE_URL_VAR).is_ok_and(|url| !url.trim().is_empty());

        if is_set {
            return Some(common::test_database_url());
        }

        eprintln!(
            "skipping {test}: {} is not set, so there is no disposable database to start the \
             server against",
            common::TEST_DATABASE_URL_VAR
        );

        None
    }

    /// Reserves a free loopback port.
    fn free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free loopback port is available");
        let port = listener
            .local_addr()
            .expect("the reserved socket has an address")
            .port();
        drop(listener);
        port
    }

    /// Sends one request and returns the status line and headers.
    fn http_get(addr: SocketAddr, path: &str) -> Option<String> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("the read timeout can be set");
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("the write timeout can be set");

        let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).ok()?;

        read_head(&mut stream)
    }

    /// Starts a request whose body is never sent.
    ///
    /// Returns the socket, which the caller must keep alive, and everything the
    /// server answered with. `Expect: 100-continue` makes the state observable:
    /// the interim response is only written once the handler asks for the body,
    /// which is exactly when the request is in flight.
    fn start_stalled_request(addr: SocketAddr) -> (TcpStream, String) {
        let mut stream =
            TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("the server accepts");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("the read timeout can be set");
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("the write timeout can be set");

        let head = format!(
            "POST /api/v1/auth/login HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: 4096\r\n\
             Expect: 100-continue\r\n\
             \r\n"
        );
        stream
            .write_all(head.as_bytes())
            .expect("the request head is written");
        stream.flush().expect("the request head is flushed");

        let interim = read_head(&mut stream).unwrap_or_default();

        (stream, interim)
    }

    /// Reads until the end of the headers, an end of stream, or a timeout.
    fn read_head(stream: &mut TcpStream) -> Option<String> {
        const CEILING: usize = 16 * 1024;

        let mut buffer = Vec::new();
        let mut chunk = [0u8; 256];

        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    buffer.extend_from_slice(&chunk[..read]);
                    if String::from_utf8_lossy(&buffer).contains("\r\n\r\n") {
                        break;
                    }
                }
                // Interrupted is not an answer; anything else means the peer
                // said nothing in time or closed on us.
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }

            if buffer.len() >= CEILING {
                break;
            }
        }

        if buffer.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&buffer).to_string())
        }
    }
}
