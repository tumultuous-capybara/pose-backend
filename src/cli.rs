use serde::{Serialize, Deserialize, de::DeserializeOwned};
use parse::{Value, parse_value};
use anyhow::{anyhow, Context, Result};
use std::os::unix::net::{UnixStream, UnixListener};
use clap::{ArgMatches};
use std::io::{self, BufReader, BufRead, Read, Write};
use crate::parse;

use std::time::{Duration, SystemTime};

use std::thread;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock, mpsc
};

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
    Echo(String),
}

#[derive(Serialize, Deserialize)]
enum CliResponse {
    Status,
    DatabaseTestResponse(u128),
    Echo(String),
}

fn send_json(v: impl Serialize, mut stream: &UnixStream) -> Result<()> {
    let serialized = serde_json::to_string(&v)?;
    stream.write(serialized.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn get_json<'a, T: DeserializeOwned>(stream: &UnixStream) -> Result<T> {
    let data: Result<Vec<u8>, _> = stream
        .bytes()
        .take_while(|b| b.is_ok() && *b.as_ref().unwrap() != b'\n')
        .collect();
    let data = data?;
    let res = serde_json::from_slice(&data)?;
    Ok(res)
}

pub fn receive_command(stream: UnixStream, db: &Arc<RwLock<Pool<SqliteConnection>>>) {
    let query = get_json(&stream);
    if query.is_ok() {
        let query = query.unwrap();
        match query {
            CliCommand::GetStatus => {
                match send_json(CliResponse::Status, &stream) {
                    Ok(_) => info!("status request handled"),
                    Err(_) => error!("error handling status command")
                }
            }
            CliCommand::Echo(s) => {
                match send_json(CliResponse::Echo(s), &stream) {
                    Ok(_) => info!("echo request handled"),
                    Err(_) => error!("error handling echo command")
                }
            }
            CliCommand::DatabaseTest => {
                let t1 = SystemTime::now();
                let con = db.read().unwrap();

                for n in 0i64..10000i64 {
                    sqlx::query(
                        "insert into accessTokens (key, userId) VALUES (?1, ?2)"
                    ).bind("foo").bind(0i64).execute(&*con).await.unwrap();
                }

                let elapsed: u128 = t1.elapsed().unwrap().as_millis();

                match send_json(CliResponse::DatabaseTestResponse(elapsed), &stream) {
                    Ok(_) => info!("database benchmark performed"),
                    Err(_) => error!("error handling test command")
                }
            }
            _ => info!("unhandled command")
        }
    }
}

pub fn client_mode(m: ArgMatches, path: &str) {
    let stream = UnixStream::connect(path);

    if stream.is_err() {
        println!("[Error] No active server found.");
        std::process::exit(1);
    }

    let stream = stream.unwrap();
    let subcommand = m.subcommand.unwrap();

    match subcommand.name.as_str() {
        "status" => {
            send_json(CliCommand::GetStatus, &stream).unwrap();
            let r: CliResponse = get_json(&stream).unwrap();
            println!("Server Status: Active!");
        }
        "echo" => {
            send_json(&CliCommand::Echo(subcommand.matches.value_of("input").unwrap().to_string()), &stream).unwrap();
            let r: CliResponse = get_json(&stream).unwrap();
            match r {
                CliResponse::Echo(s) => {
                    println!("Echo String Recieved: {}", s);
                }
                _ => error!("oh no")
            }
        }
        "parse" => {
            let v: &str = subcommand.matches.value_of("input").unwrap();
            let p = parse_value(v);
            match p {
                Ok(r) => println!("Value: {:?}", r.1),
                Err(e) => println!("{}", e),
            }
        }
        "test" => {
            send_json(CliCommand::DatabaseTest, &stream).unwrap();
            let r: CliResponse = get_json(&stream).unwrap();
            match r {
                CliResponse::DatabaseTestResponse(elapsed) => {
                    println!("Database benchmark complete in: {} ms", elapsed);
                }
                _ => error!("oh no")
            }
        }
        _ => error!("unrecognized subcommand")
    }
}

pub fn start_listener (path: String, db: Arc<RwLock<Pool<SqliteConnection>>>) {
    thread::spawn(move || match UnixListener::bind(path) {
        Ok(listener) => {
            info!("Unix Socket established");
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        stream.set_read_timeout(Some(Duration::new(1, 0)))
                            .expect("Couldn't set read timeout!");
                        receive_command(stream, &db);
                    }
                    Err(err) => {
                        error!("IPC connection error: {}", err);
                    }
                }
            }
        }
        Err(err) => error!("Cannot start IPC {}", err),
    });
}
