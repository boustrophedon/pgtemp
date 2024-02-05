use std::process::{Child, Command};
use tempfile::TempDir;

use crate::PgTempDBBuilder;

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

    // postgres will not run as root, so try to run initdb as postgres user if we are root so that
    // when running the server as the postgres user it can access the files
    let mut cmd: Command;
    if current_user_is_root() {
        cmd = Command::new("sudo");
        cmd.args(["-u", "postgres"]).arg("initdb");
    } else {
        cmd = Command::new("initdb");
    }

    cmd.args(["-D", data_dir_str])
        .arg("-N") // no fsync, starts slightly faster
        .args(["--username", &user])
        .args(["--pwfile", pwfile_str]);

    #[cfg(feature = "pg16")]
    {
        let port = builder.get_port_or_set_random();
        cmd.args(["-c", &format!("unix_socket_directories={}", data_dir_str)])
            .args(["-c", &format!("port={port}")]);
    }

    #[cfg(feature = "pg16")]
    for (key, val) in &builder.server_configs {
        cmd.args(["-c", &format!("{}={}", key, val)]);
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
    let mut pgcmd: Command;
    if current_user_is_root() {
        pgcmd = Command::new("sudo");
        pgcmd.args(["-u", "postgres"]).arg("postgres");
    } else {
        pgcmd = Command::new("postgres");
    };

    pgcmd
        .args(["-c", &format!("unix_socket_directories={}", data_dir_str)])
        .args(["-c", &format!("port={port}")])
        .arg("-F") // no fsync for faster setup and execution
        .args(["-D", data_dir.to_str().unwrap()]);

    // don't output postgres output to stdout/stderr
    pgcmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let postgres_server_process = pgcmd
        .spawn()
        .expect("Failed to start postgres. Is it installed and on your path?");

    // TODO: read from postgres stderr says "ready to accept connections"
    // or just loop on tcp connecting to db?
    std::thread::sleep(std::time::Duration::from_millis(100));

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
        let mut dbcmd = Command::new("createdb");
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
        let createdb_output = dbcmd.output().expect("Failed to start createdb. Is it installed and on your path? It's typically part of the postgres-libs or postgres-client package.");

        if !createdb_output.status.success() {
            let stdout = createdb_output.stdout;
            let stderr = createdb_output.stderr;
            panic!(
                "createdb failed! stdout: {}\n\nstderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    postgres_server_process
}
