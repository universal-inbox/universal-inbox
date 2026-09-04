//! Test-only database provisioning.
//!
//! Isolation model: each test leases one *slot* database, held for the test's lifetime by a
//! session-scoped `pg_advisory_lock`. Slot databases are provisioned once per run from a
//! template built from the embedded migrations, then reset between tests with `TRUNCATE`.
//!
//! Two properties fall out of that:
//!
//! * The number of test databases is bounded by [`slot_count`], forever — there is nothing to
//!   clean up periodically.
//! * A killed test (SIGKILL, ctrl-C, nextest timeout) needs no teardown: Postgres releases a
//!   session advisory lock when the socket closes, and the next lease resets the database
//!   before use.
//!
//! Migrations are replayed once per template, not once per test.

use std::{sync::Arc, time::Duration};

use sqlx::{Connection, Executor, PgConnection, PgPool};

use universal_inbox_api::configuration::DatabaseSettings;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const DB_PREFIX: &str = "ui_test";
/// `LIKE` patterns for the sweep. `\_` escapes the underscore wildcard.
const TEMPLATE_LIKE: &str = r"ui\_test\_tmpl\_%";

/// Advisory lock guarding template creation (single-argument form).
const TEMPLATE_LOCK_KEY: i64 = 0x5549_5F54_4D50_4C00;
/// Advisory lock namespace for slot leases (two-argument form).
const SLOT_LOCK_NAMESPACE: i32 = 0x5549_5F53;

/// Attempts at `CREATE DATABASE ... TEMPLATE`, which can lose a race against autovacuum.
const CREATE_ATTEMPTS: u32 = 3;

/// A leased test database. Dropping it releases the slot.
pub struct TestDb {
    pub pool: Arc<PgPool>,
    #[allow(dead_code)]
    pub database_name: String,
    /// Holds the slot's advisory lock for as long as this value lives. Never used directly:
    /// closing the socket — including by dying — is what releases the lock.
    #[allow(dead_code)]
    lease: PgConnection,
}

/// Provisions a pristine database for the calling test and returns a pool onto it.
///
/// `settings.database_name` is rewritten to the leased slot, so the pool the caller gets is
/// built by the same [`DatabaseSettings::connect_pool`] the server uses in production.
pub async fn acquire(settings: &mut DatabaseSettings) -> TestDb {
    let template = template_name();
    ensure_template(settings, &template).await;

    let (index, mut lease) = lease_slot(settings).await;
    let database_name = format!("{DB_PREFIX}_slot_{index}");

    // Escape hatch: `UI_TEST_DB_RESET=recreate` forces a fresh clone per test instead of a
    // `TRUNCATE`, to rule the reset strategy in or out when a cross-test leak is suspected.
    let recreate = std::env::var("UI_TEST_DB_RESET").as_deref() == Ok("recreate");
    if recreate {
        drop_database(&mut lease, &database_name).await;
        create_from_template(&mut lease, &database_name, &template).await;
        stamp_database(settings, &database_name, &template).await;
    } else {
        ensure_slot_database(settings, &mut lease, &database_name, &template).await;
    }

    settings.database_name = database_name.clone();
    let pool = settings
        .connect_pool(log::LevelFilter::Info)
        .await
        .expect("Failed to connect to the test database");

    if !recreate {
        reset(&pool).await;
    }

    TestDb {
        pool: Arc::new(pool),
        database_name,
        lease,
    }
}

// ---------------------------------------------------------------------------
// Template
// ---------------------------------------------------------------------------

/// Template name derived from the *content* of the embedded migrations, so adding or editing
/// a migration renames the template and rebuilds it automatically.
fn template_name() -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for migration in MIGRATOR.iter() {
        hasher.update(migration.version.to_le_bytes());
        hasher.update(&migration.checksum);
    }
    format!("{DB_PREFIX}_tmpl_{}", &hex::encode(hasher.finalize())[..12])
}

async fn ensure_template(settings: &DatabaseSettings, template: &str) {
    let mut connection = maintenance_connection(settings).await;
    if database_exists(&mut connection, template).await {
        return;
    }

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(TEMPLATE_LOCK_KEY)
        .execute(&mut connection)
        .await
        .expect("Failed to take the template advisory lock");

    // Re-check under the lock: a concurrent process may have built it while we waited.
    if !database_exists(&mut connection, template).await {
        build_template(settings, &mut connection, template).await;
    }
    sweep_stale_templates(&mut connection, template).await;

    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(TEMPLATE_LOCK_KEY)
        .execute(&mut connection)
        .await;
    let _ = connection.close().await;
}

/// Builds into a scratch name and renames on success, so a process killed mid-build leaves a
/// `..._building` leftover (swept later) rather than a half-migrated template.
async fn build_template(
    settings: &DatabaseSettings,
    connection: &mut PgConnection,
    template: &str,
) {
    let building = format!("{template}_building");
    drop_database(connection, &building).await;
    connection
        .execute(&*format!(r#"CREATE DATABASE "{building}";"#))
        .await
        .expect("Failed to create the template database");

    let mut build_settings = settings.clone();
    build_settings.database_name = building.clone();
    let mut build_connection = PgConnection::connect(&build_settings.connection_string())
        .await
        .expect("Failed to connect to the template database");

    MIGRATOR
        .run(&mut build_connection)
        .await
        .expect("Failed to migrate the template database");

    // Leave autovacuum nothing to do. An autovacuum worker connected to the template makes a
    // concurrent `CREATE DATABASE ... TEMPLATE` fail with SQLSTATE 55006, and `datallowconn`
    // does not keep autovacuum out.
    build_connection
        .execute("VACUUM FREEZE")
        .await
        .expect("Failed to freeze the template database");
    build_connection
        .execute("ANALYZE")
        .await
        .expect("Failed to analyze the template database");
    // Explicit, not `drop`: `ALTER DATABASE ... RENAME TO` fails while a connection remains.
    build_connection
        .close()
        .await
        .expect("Failed to close the template build connection");

    connection
        .execute(&*format!(
            r#"ALTER DATABASE "{building}" RENAME TO "{template}";"#
        ))
        .await
        .expect("Failed to rename the template database");
    connection
        .execute(&*format!(
            r#"ALTER DATABASE "{template}" IS_TEMPLATE true;"#
        ))
        .await
        .expect("Failed to flag the template database");
    connection
        .execute(&*format!(
            r#"ALTER DATABASE "{template}" ALLOW_CONNECTIONS false;"#
        ))
        .await
        .expect("Failed to seal the template database");
}

/// Drops every template but the current one. This is what makes a periodic manual cleanup
/// command unnecessary: an obsolete template goes away on the first run that needs a new one.
async fn sweep_stale_templates(connection: &mut PgConnection, keep: &str) {
    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT datname FROM pg_database WHERE datname LIKE $1 AND datname <> $2",
    )
    .bind(TEMPLATE_LIKE)
    .bind(keep)
    .fetch_all(&mut *connection)
    .await
    .unwrap_or_default();

    for database in stale {
        // Postgres refuses to drop a database still flagged as a template.
        let _ = connection
            .execute(&*format!(
                r#"ALTER DATABASE "{database}" IS_TEMPLATE false;"#
            ))
            .await;
        drop_database(connection, &database).await;
    }
}

// ---------------------------------------------------------------------------
// Slots
// ---------------------------------------------------------------------------

/// Hard upper bound on how many test databases can ever exist. Doubled so a second concurrent
/// run (a stray `cargo test`, a browser run alongside an API run) does not queue behind the first.
fn slot_count() -> i32 {
    if let Some(count) = env_i32("UI_TEST_SLOTS") {
        return count.max(1);
    }
    if let Some(threads) = env_i32("NEXTEST_TEST_THREADS") {
        return (threads * 2).max(1);
    }
    std::thread::available_parallelism()
        .map(|threads| threads.get() as i32 * 2)
        .unwrap_or(16)
}

/// Leases a slot, returning its index and the connection holding the lock.
///
/// `NEXTEST_TEST_GLOBAL_SLOT` is unique among concurrently running tests, so it is used as the
/// starting probe to spread contention — but correctness rests on the advisory lock, which also
/// covers plain `cargo test` and two simultaneous runs.
async fn lease_slot(settings: &DatabaseSettings) -> (i32, PgConnection) {
    let count = slot_count();
    let start = env_i32("NEXTEST_TEST_GLOBAL_SLOT")
        .map(|slot| slot.rem_euclid(count))
        .unwrap_or(0);

    let mut connection = maintenance_connection(settings).await;
    for offset in 0..count {
        let index = (start + offset) % count;
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind(SLOT_LOCK_NAMESPACE)
            .bind(index)
            .fetch_one(&mut connection)
            .await
            .expect("Failed to probe a test database slot");
        if acquired {
            return (index, connection);
        }
    }

    // More concurrent tests than slots: queue rather than fail.
    sqlx::query("SELECT pg_advisory_lock($1, $2)")
        .bind(SLOT_LOCK_NAMESPACE)
        .bind(start)
        .execute(&mut connection)
        .await
        .expect("Failed to wait for a test database slot");
    (start, connection)
}

/// Creates the slot database from the template unless it is already a clone of it.
async fn ensure_slot_database(
    settings: &DatabaseSettings,
    connection: &mut PgConnection,
    database: &str,
    template: &str,
) {
    let stamp: Option<String> = sqlx::query_scalar(
        "SELECT shobj_description(oid, 'pg_database') FROM pg_database WHERE datname = $1",
    )
    .bind(database)
    .fetch_optional(&mut *connection)
    .await
    .expect("Failed to read the test database stamp")
    .flatten();

    if stamp.as_deref() == Some(template) {
        // A previous run may have died holding connections; they would block the reset.
        terminate_backends(connection, database).await;
        return;
    }

    drop_database(connection, database).await;
    create_from_template(connection, database, template).await;
    stamp_database(settings, database, template).await;
}

async fn create_from_template(connection: &mut PgConnection, database: &str, template: &str) {
    // No `STRATEGY` clause on purpose: the default `WAL_LOG` is the right choice for a small
    // database, while `FILE_COPY` forces two cluster-wide immediate checkpoints per clone.
    let statement = format!(r#"CREATE DATABASE "{database}" TEMPLATE "{template}";"#);

    for attempt in 1..=CREATE_ATTEMPTS {
        match connection.execute(&*statement).await {
            Ok(_) => return,
            // 55006 object_in_use: an autovacuum worker beat us to the template.
            Err(error) if attempt < CREATE_ATTEMPTS && is_object_in_use(&error) => {
                tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
            }
            Err(error) => {
                panic!("Failed to create {database} from template {template}: {error}")
            }
        }
    }
}

/// Records which template the slot was cloned from, so the next lease can skip recreating it.
/// Requires connecting to the database: Postgres only allows commenting on the current one.
async fn stamp_database(settings: &DatabaseSettings, database: &str, template: &str) {
    let mut stamp_settings = settings.clone();
    stamp_settings.database_name = database.to_string();
    let mut connection = PgConnection::connect(&stamp_settings.connection_string())
        .await
        .expect("Failed to connect to the slot database");
    connection
        .execute(&*format!(
            r#"COMMENT ON DATABASE "{database}" IS '{template}';"#
        ))
        .await
        .expect("Failed to stamp the slot database");
    let _ = connection.close().await;
}

// ---------------------------------------------------------------------------
// Reset
// ---------------------------------------------------------------------------

/// One statement, ~milliseconds. Safe as a full reset here because the schema has no state to
/// re-seed: the migrations' `INSERT`s are all backfills that touch zero rows on a fresh
/// database, there are no extensions or sequences, and no test performs DDL.
const RESET_STATEMENT: &str = r#"
DO $$
DECLARE
    truncate_statement text;
BEGIN
    SELECT 'TRUNCATE TABLE '
           || string_agg(format('%I.%I', schemaname, tablename), ', ')
           || ' RESTART IDENTITY CASCADE'
      INTO truncate_statement
      FROM pg_tables
     WHERE schemaname = 'public'
       AND tablename <> '_sqlx_migrations';

    IF truncate_statement IS NOT NULL THEN
        EXECUTE truncate_statement;
    END IF;
END $$;
"#;

async fn reset(pool: &PgPool) {
    pool.execute(RESET_STATEMENT)
        .await
        .expect("Failed to reset the test database");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn maintenance_connection(settings: &DatabaseSettings) -> PgConnection {
    PgConnection::connect(&settings.connection_string_without_db())
        .await
        .expect("Failed to connect to Postgres")
}

async fn database_exists(connection: &mut PgConnection, database: &str) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM pg_database WHERE datname = $1")
        .bind(database)
        .fetch_optional(&mut *connection)
        .await
        .expect("Failed to look up the database")
        .is_some()
}

async fn drop_database(connection: &mut PgConnection, database: &str) {
    connection
        .execute(&*format!(
            r#"DROP DATABASE IF EXISTS "{database}" WITH (FORCE);"#
        ))
        .await
        .unwrap_or_else(|error| panic!("Failed to drop database {database}: {error}"));
}

async fn terminate_backends(connection: &mut PgConnection, database: &str) {
    let _ = sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = $1 AND pid <> pg_backend_pid()",
    )
    .bind(database)
    .execute(&mut *connection)
    .await;
}

fn is_object_in_use(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "55006")
}

fn env_i32(name: &str) -> Option<i32> {
    std::env::var(name).ok()?.parse().ok()
}
