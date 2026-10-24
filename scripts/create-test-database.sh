#!/bin/sh
# Creates the local databases the compose file needs beyond `POSTGRES_DB`.
#
# The official PostgreSQL image runs the scripts in
# /docker-entrypoint-initdb.d/ exactly once, when the data directory is
# initialised. The image creates one database from `POSTGRES_DB`; the local
# setup needs two more, and both must exist from the start:
#
#   atom_test  the disposable database the integration suite runs against; the
#              harness refuses a name that does not contain "test".
#   atom_dev   the development database the API and the seeder point at; the
#              seeder refuses a name that is not a development name, and
#              `atom` is one of the names it refuses.
#
# The seeder's guard is deliberate and must not be weakened, so the local setup
# provides a database the guard already accepts instead of asking a developer
# to rename one.
#
# This script is a no-op when a database already exists.

set -e

TEST_DB="${TEST_DB:-atom_test}"
DEV_DB="${DEV_DB:-atom_dev}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    SELECT 'CREATE DATABASE $TEST_DB OWNER $POSTGRES_USER'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '$TEST_DB')\gexec
EOSQL

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    SELECT 'CREATE DATABASE $DEV_DB OWNER $POSTGRES_USER'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '$DEV_DB')\gexec
EOSQL
