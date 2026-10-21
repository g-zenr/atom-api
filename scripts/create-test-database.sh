#!/bin/sh
# Creates the disposable test database used by the integration suite.
#
# The official PostgreSQL image runs the scripts in
# /docker-entrypoint-initdb.d/ exactly once, when the data directory is
# initialised. The integration suite refuses to run against a database whose
# name does not contain "test", so the database must exist from the start.
#
# This script is a no-op when the database already exists.

set -e

TEST_DB="${TEST_DB:-atom_test}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    SELECT 'CREATE DATABASE $TEST_DB OWNER $POSTGRES_USER'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '$TEST_DB')\gexec
EOSQL
