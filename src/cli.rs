use serde::{Serialize, Deserialize, de::DeserializeOwned};
use parse::{Value, parse_value};
use anyhow::{anyhow, Context, Result};
use clap::{ArgMatches};
use std::io::{Read, Write};
use crate::parse;

use std::thread;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc
};

use tokio::sync::broadcast;
use tokio::sync::broadcast::{Sender, Receiver};

use tokio::net::UnixListener;
use tokio::net::UnixStream;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::io::AsyncBufReadExt;

use sqlx::sqlite::SqlitePool;
use sqlx::Executor;
use sqlx::pool::PoolConnection;
use sqlx::SqliteConnection;
use sqlx::Sqlite;
use sqlx::Pool;

#[derive(Serialize, Deserialize)]
enum CliCommand {
    GetStatus,
    Stop,
    DatabaseTest,
}

#[derive(Serialize, Deserialize)]
enum CliResponse {
    Status,
    DatabaseTestResponse(u128),
    StoppingServer,
}

async fn send_json(v: impl Serialize, stream: &mut UnixStream) -> Result<()> {
    let serialized = serde_json::to_string(&v)?;
    stream.write(serialized.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

async fn get_json<'a, T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let mut data = BufReader::with_capacity(512, stream);
    let mut msg = String::new();
    data.read_line(&mut msg).await?;
    let res = serde_json::from_str(&msg)?;
    Ok(res)
}

async fn receive_command(mut stream: UnixStream, db: &Arc<Pool<Sqlite>>, shutdown_writer: &Sender<()>) {
    let query = get_json(&mut stream).await;
    if query.is_ok() {
        let query = query.unwrap();
        match query {
            CliCommand::GetStatus => {
                match send_json(CliResponse::Status, &mut stream).await {
                    Ok(_) => info!("status request handled"),
                    Err(_) => error!("error handling status command")
                }
            }
            CliCommand::Stop => {
                match send_json(CliResponse::StoppingServer, &mut stream).await {
                    Ok(_) => info!("recieved stop command"),
                    Err(_) => error!("error handling status command")
                }
                shutdown_writer.send(()).ok();
            }
            // CliCommand::DatabaseTest => {
            //     let t1 = SystemTime::now();
            //
            //     for n in 0i64..10000i64 {
            //         sqlx::query(
            //             "insert into accessTokens (key, userId) VALUES (?1, ?2)"
            //         ).bind("foo").bind(0i64).execute(&**db).await.unwrap();
            //     }
            //
            //     let elapsed: u128 = t1.elapsed().unwrap().as_millis();
            //
            //     match send_json(CliResponse::DatabaseTestResponse(elapsed), &stream) {
            //         Ok(_) => info!("database benchmark performed"),
            //         Err(_) => error!("error handling test command")
            //     }
            // }
            _ => info!("unhandled command")
        }
    }
}

pub async fn client_mode(query: ArgMatches<'_>, path: &str) {
    let subcommand = query.subcommand.unwrap();

    // some dev commands don't require a socket or server
    match subcommand.name.as_str() {
        "parse" => {
            let v: &str = subcommand.matches.value_of("input").unwrap();
            let p = parse_value(v);
            match p {
                Ok(r) => println!("Value: {:?}", r.1),
                Err(e) => println!("{}", e),
            }
            return ();
        }
        _ => { }
    }

    let stream = UnixStream::connect(path).await;

    if stream.is_err() {
        println!("[Error] No active server found.");
        std::process::exit(1);
    }

    let mut stream = stream.unwrap();

    match subcommand.name.as_str() {
        "status" => {
            send_json(CliCommand::GetStatus, &mut stream).await.unwrap();
            let r: CliResponse = get_json(&mut stream).await.unwrap();
            println!("Server Status: Active!");
        }
        "stop" => {
            send_json(CliCommand::Stop, &mut stream).await.unwrap();
            let r: CliResponse = get_json(&mut stream).await.unwrap();
            println!("Stopping Server!");
        }
        // "test" => {
        //     send_json(CliCommand::DatabaseTest, &stream).unwrap();
        //     let r: CliResponse = get_json(&stream).unwrap();
        //     match r {
        //         CliResponse::DatabaseTestResponse(elapsed) => {
        //             println!("Database benchmark complete in: {} ms", elapsed);
        //         }
        //         _ => error!("oh no")
        //     }
        // }
        _ => error!("unrecognized subcommand")
    }
}

pub async fn start_listener(shutdown_writer: Sender<()>, path: String, db: Arc<Pool<Sqlite>>) {
    let mut shutdown_reader = shutdown_writer.subscribe();
    match UnixListener::bind(path) {
        Ok(listener) => {
            info!("Unix Socket established");
            loop {
                tokio::select! {
                    _ = shutdown_reader.recv() => {
                        break;
                    }
                    connection = listener.accept() => {
                        match connection {
                            Ok((stream, _addr)) => {
                                receive_command(stream, &db, &shutdown_writer).await;
                            }
                            Err(err) => {
                                println!("IPC connection error: {}", err);
                            }
                        }
                    }
                }
            }
        }
        Err(err) => println!("Cannot start IPC {}", err),
    }
}
