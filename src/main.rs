#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(non_upper_case_globals)]
#![feature(option_insert)]

#[macro_use]
mod macros;

#[macro_use]
extern crate clap;

mod parse;
mod cli;

use parse::{Value, parse_value};
use anyhow::{anyhow, Context, Result};

use clap::{Arg, App, SubCommand, ArgMatches};

use std::os::unix::net::{UnixStream, UnixListener};
use std::thread;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock, mpsc
};
use std::str;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;
use std::io::{self, BufReader, BufRead, Read, Write};
use std::fs;

use rand::{thread_rng, Rng};

use futures::{FutureExt, StreamExt};
use warp::ws::{Message, WebSocket};
use warp::{Filter, http::Response};

use sqlx::sqlite::SqlitePool;
use sqlx::Executor;

static index: &'static [u8] = include_bytes!("../mock-frontend/index.html");
static js: &'static [u8] = include_bytes!("../mock-frontend/main.js");
static css: &'static [u8] = include_bytes!("../mock-frontend/main.css");

type AcessTokens = Arc<RwLock<Vec<[u8; 64]>>>;

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
            (@subcommand echo =>
                (about: "Echos back a response.")
                (@arg input: +required "String to echo."))
            (@subcommand parse =>
                (about: "Parses some code.")
                (@arg input: +required "String to parse."))
            (@subcommand test =>
                (about: "Runs one of several test and benchmarking options.")
                (@arg input: +required "Test to run."))
        ).get_matches();

    // configurable parameters

    let port: u16 = matches.value_of("port").map_or(80, |x| x.parse().unwrap());
    let db_path = matches.value_of("db").unwrap_or("sqlite://./pose.db");
    let socket_path = matches.value_of("socket").unwrap_or("/tmp/pose.socket");

    // this is a client instance of pose, and does not start the server
    if matches.subcommand.is_some() {
        cli::client_mode(matches.clone(), socket_path);
        std::process::exit(0);
    }

    // the server is already started at this socket!
    if UnixStream::connect(&socket_path).is_ok() {
        println!("[Error] Server is already active!");
        std::process::exit(1);
    }

    let tokens = AcessTokens::default();

    // database setup
    let database =
        Arc::new(RwLock::new(
            SqlitePool::builder()
                .max_size(8)
                .min_size(4)
                .build(&db_path)
                .await.unwrap()));
    info!("database connection established");
    {
        let con = database.read().unwrap();
        sqlx::query(
            "begin;
                create table if not exists accessTokens (
                    key     text      primary key not null,
                    userId  integer   not null
                );
                create table if not exists users (
                    id      integer   primary key not null
                );
            commit;"
        ).execute(&*con).await.unwrap();
    }

    // start the console control listener
    if fs::metadata(socket_path).is_ok() {
        match fs::remove_file(socket_path) {
            Ok(v)    => info!("old socket file cleared"),
            Err(err) => error!("can't remove file: {}", err)
        }
    }

    cli::start_listener(socket_path.to_string(), database.clone());

    // ** Warp Routes **
    // <static files>                -- The frontend resources, html, css, js, and woff2
    //
    // api/auth/authenticate {token} -- Attempts to use a token to auth the connection
    // api/auth/login {user, pass}   -- Makes a login attempt, returns a new token if successful
    // api/auth/register {id}        -- Accepts an invite
    // api/invite {email}            -- Sends an invite request to the specified email
    // invite/<id>                   -- External link for requested invite, handled client-side

    let index_route = warp::path::end().map(|| {
        warp::reply::html(str::from_utf8(index).unwrap())
    });
    let js_route = warp::path!("main.js").map(|| {
        Response::builder()
            .header("content-type", "text/javascript; charset=utf-8")
            .body(str::from_utf8(js).unwrap())
    });
    let css_route = warp::path!("main.css").map(|| {
        Response::builder()
            .header("content-type", "text/css; charset=utf-8")
            .body(str::from_utf8(css).unwrap())
    });
    let css_route = warp::path!("main.css").map(|| {
        Response::builder()
            .header("content-type", "text/css; charset=utf-8")
            .body(str::from_utf8(css).unwrap())
    });


    let routes = index_route.or(js_route).or(css_route);

    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}
