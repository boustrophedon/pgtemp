use std::{
    process::{Child, Command},
    time::Duration,
};
use tempfile::TempDir;

use crate::PgTempDBBuilder;

const CREATEDB_MAX_TRIES: u32 = 10;
const CREATEDB_RETRY_DELAY: Duration = Duration::from_millis(100);

fn current_user_is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Execute the `initdb` binary with the parameters configured in PgTempDBBuilder.
pub fn init_db(builder: &mut PgTempDBBuilder) -> TempDir {
    let temp_dir = {
        if let Some(base_dir) = builder.temp_dir_prefix.clone() {
            TempDir::with_prefix_in("pgtemp-", base_dir).expect("failed to create tempdir")
        } else {
            TempDir::with_prefix("pgtemp-").expect("failed to create tempdir")
        }
    };

    // if current user is root, data dir etc need to be owned by postgres user
    if current_user_is_root() {
        // TODO: don't shell out to chown, get the userid of postgres and just call std::os
        let chown_output = Command::new("chown")
            .args(["-R", "postgres", temp_dir.path().to_str().unwrap()])
            .output()
            .expect("failed to chown data dir to postgres user");
        if !chown_output.status.success() {
            let stdout = chown_output.stdout;
            let stderr = chown_output.stderr;
            panic!(
                "chowning data dir failed! stdout: {}\n\nstderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    let data_dir = temp_dir.path().join("pg_data_dir");
    let data_dir_str = data_dir.to_str().unwrap();

    let user = builder.get_user();
    let password = builder.get_password();

    // write out password file for initdb
    let pwfile = temp_dir.path().join("user_password.txt");
    let pwfile_str = pwfile.to_str().unwrap();
    std::fs::write(&pwfile, password).expect("failed to write password file");

    let initdb_path = builder
        .bin_path
        .as_ref()
        .map_or("initdb".into(), |p| p.join("initdb"));

    // postgres will not run as root, so try to run initdb as postgres user if we are root so that
    // when running the server as the postgres user it can access the files
    let mut cmd: Command;
    if current_user_is_root() {
        cmd = Command::new("sudo");
        cmd.args(["-u", "postgres"]).arg(initdb_path);
    } else {
        cmd = Command::new(initdb_path);
    }

    cmd.args(["-D", data_dir_str])
        .arg("-N") // no fsync, starts slightly faster
        .args(["--username", &user])
        .args(["--pwfile", pwfile_str]);

    // Apply any custom initdb configurations
    for (key, val) in &builder.initdb_args {
        // Don't add -- prefix if the key already starts with - or --
        let formatted_key = if key.starts_with('-') {
            key.to_string()
        } else {
            format!("--{}", key)
        };
        cmd.args([formatted_key.as_str(), val]);
    }

    // TODO: supply postgres install location in builder struct
    let initdb_output = cmd
        .output()
        .expect("Failed to start initdb. Is it installed and on your path?");

    if !initdb_output.status.success() {
        let stdout = initdb_output.stdout;
        let stderr = initdb_output.stderr;
        panic!(
            "initdb failed! stdout: {}\n\nstderr: {}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }

    temp_dir
}

pub fn run_db(temp_dir: &TempDir, mut builder: PgTempDBBuilder) -> Child {
    let data_dir = temp_dir.path().join("pg_data_dir");
    let data_dir_str = data_dir.to_str().unwrap();
    let port = builder.get_port_or_set_random();

    // postgres will not run as root, so try to run as postgres if we are root
    let postgres_path = builder
        .bin_path
        .as_ref()
        .map_or("postgres".into(), |p| p.join("postgres"));
    let mut pgcmd: Command;
    if current_user_is_root() {
        pgcmd = Command::new("sudo");
        pgcmd.args(["-u", "postgres"]).arg(postgres_path);
    } else {
        pgcmd = Command::new(postgres_path);
    };

    pgcmd
        .args(["-c", &format!("unix_socket_directories={}", data_dir_str)])
        .args(["-c", &format!("port={port}")])
        // https://www.postgresql.org/docs/current/non-durability.html
        // https://wiki.postgresql.org/wiki/Tuning_Your_PostgreSQL_Server
        .args(["-c", "fsync=off"])
        .args(["-c", "synchronous_commit=off"])
        .args(["-c", "full_page_writes=off"])
        .args(["-c", "autovacuum=off"])
        .args(["-D", data_dir.to_str().unwrap()]);
    for (key, val) in &builder.server_configs {
        pgcmd.args(["-c", &format!("{}={}", key, val)]);
    }

    // don't output postgres output to stdout/stderr
    pgcmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let postgres_server_process = pgcmd
        .spawn()
        .expect("Failed to start postgres. Is it installed and on your path?");

    std::thread::sleep(CREATEDB_RETRY_DELAY);

    let user = builder.get_user();
    //let password = builder.get_password();
    let port = builder.get_port_or_set_random();
    let dbname = builder.get_dbname();

    if dbname != "postgres" {
        // TODO: don't use createdb, connect directly to the db and run CREATE DATABASE. removes
        // dependency on OS package which is often separate from postgres server package (at
        // expense of adding Cargo dependency)
        //
        // alternatively just use psql
        let createdb_path = builder
            .bin_path
            .as_ref()
            .map_or("createdb".into(), |p| p.join("createdb"));
        let mut createdb_last_error_output = None;

        for _ in 0..CREATEDB_MAX_TRIES {
            let mut dbcmd = Command::new(createdb_path.clone());
            dbcmd
                .args(["--host", "localhost"])
                .args(["--port", &port.to_string()])
                .args(["--username", &user])
                // TODO: use template in pgtemp daemon single-cluster mode?
                //.args(["--template", "..."]
                // TODO: since we trust local users by default we don't actually
                // need the password but we should provide it anyway since we
                // provide it everywhere else
                .arg("--no-password")
                .arg(&dbname);

            let output = dbcmd.output().expect("Failed to start createdb. Is it installed and on your path? It's typically part of the postgres-libs or postgres-client package.");
            if output.status.success() {
                createdb_last_error_output = None;
                break;
            }
            createdb_last_error_output = Some(output);
            std::thread::sleep(CREATEDB_RETRY_DELAY);
        }

        if let Some(output) = createdb_last_error_output {
            let stdout = output.stdout;
            let stderr = output.stderr;
            panic!(
                "createdb failed! stdout: {}\n\nstderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    postgres_server_process
}
