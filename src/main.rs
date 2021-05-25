#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![feature(option_insert)]

#[macro_use]
mod macros;

#[macro_use]
extern crate clap;

mod parse;
mod cli;
mod server;

use anyhow::{anyhow, Context, Result};

use clap::{Arg, App, SubCommand, ArgMatches};

use std::{str, fs, sync::{Arc, RwLock}};
use std::os::unix::net::UnixStream;

use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Sender, Receiver};

use futures::{FutureExt, StreamExt};

use sqlx::{sqlite::SqlitePool, Executor, pool::PoolOptions, sqlite::SqlitePoolOptions};

#[tokio::main]
async fn main() {
    let matches =
        clap_app!(pose =>
            (version: "0.1")
            (author: "capybaba <tumultuous-capybara>")
            (about: "The pose backend server, ")
            (@arg port: --port +takes_value "port to start on (default 80)")
            (@arg db: --db +takes_value "file path to use for the database")
            (@arg socket: --socket +takes_value "File path to use for the socket")
            (@subcommand status =>
                (about: "Determines if the server is online and active."))
            (@subcommand stop =>
                (about: "Shuts down the server."))
            (@subcommand parse =>
                (about: "Parses some code.")
                (@arg input: +required "String to parse."))
            (@subcommand test =>
                (about: "Runs one of several test and benchmarking options.")
                (@arg input: +required "Test to run."))
        ).get_matches();

    // configurable parameters necessary to start the server
    let port: u16 = matches.value_of("port").map_or(80, |x| x.parse().unwrap());
    let db_path = matches.value_of("db").unwrap_or("sqlite://./pose.db");
    let socket_path = matches.value_of("socket").unwrap_or("/tmp/pose.socket");

    // this is a client instance of pose, and does not start the server
    if matches.subcommand.is_some() {
        cli::client_mode(matches.clone(), socket_path).await;
        std::process::exit(0);
    }

    // the server is already started at this socket!
    if UnixStream::connect(&socket_path).is_ok() {
        panic!("[Error] Server is already active!");
    }

    // initalize various structs
    let (shutdown_writer, shutdown_reader): (Sender<()>, Receiver<()>) = broadcast::channel(1);

    // listener for shutdown on unix signals (SIGTERM, SIGHUP, SIGINT)
    let shutdown_writer_unix = shutdown_writer.clone();
    let shutdown_watcher = async move {
        let mut terminate = signal(SignalKind::terminate()).unwrap();
        let mut hangup = signal(SignalKind::hangup()).unwrap();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { }
            _ = hangup.recv() => { }
            _ = terminate.recv() => { }
        }
        shutdown_writer_unix.send(()).ok();
        info!("Shutting down...");
    };

    // database setup
    let database =
        Arc::new(
            SqlitePoolOptions::new()
                .max_connections(8u32)
                .min_connections(4u32)
                .connect(&db_path)
            .await.unwrap());

    info!("database connection established");

    // create the tables if they don't exist
    {
        sqlx::query(
            "begin;
                create table if not exists accessTokens (
                    key text primary key not null,
                    userId integer not null
                );
                create table if not exists users (
                    id integer primary key not null
                );
            commit;"
        ).execute(&*database).await.unwrap();
    }

    // make sure there isn't an old socket file present
    if fs::metadata(socket_path).is_ok() {
        match fs::remove_file(socket_path) {
            Ok(v)    => info!("old socket file cleared"),
            Err(err) => panic!("can't remove file: {}", err)
        }
    }

    // start all secondary tasks
    tokio::task::spawn(shutdown_watcher);
    tokio::task::spawn(cli::start_listener(shutdown_writer.clone(), socket_path.to_string(), database.clone()));

    // start the main server
    server::start_server(database, port, shutdown_reader).await;

    // cleanup after server stop
    if fs::metadata(socket_path).is_ok() {
        match fs::remove_file(socket_path) {
            Ok(v)    => info!("cleaned up socket file"),
            Err(err) => error!("can't remove file: {}", err)
        }
    }
}
