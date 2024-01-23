use clap::Parser;

#[tokio::main]
async fn main() {
    let args = pgtemp::PgTempDaemonArgs::parse();
    pgtemp::PgTempDaemon::from_args(args).await.start().await;
}
