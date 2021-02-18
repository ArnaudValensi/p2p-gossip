#![warn(rust_2018_idioms)]

use clap::{App, Arg};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{self, Duration};

#[derive(Debug)]
enum Command {
    NewTick,
}

type Tx = mpsc::Sender<Command>;

struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}

impl Shared {
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    fn add_peer(&mut self, addr: SocketAddr) -> mpsc::Receiver<Command> {
        let (tx, rx) = mpsc::channel(32);
        self.peers.insert(addr, tx);

        rx
    }

    async fn broadcast(&mut self) -> Result<(), Box<dyn Error>> {
        for peer in self.peers.iter_mut() {
            peer.1.send(Command::NewTick).await?;
        }

        Ok(())
    }

    fn get_peer_addresses(&self) -> String {
        self.peers
            .keys()
            .map(|addr| addr.to_string())
            .collect::<Vec<String>>()
            .join(",")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = App::new("My Super Program")
        .version("1.0")
        .author("Arnaud Valensi <arnaud.valensi@gmail.com>")
        .arg(
            Arg::with_name("period")
                .long("period")
                .value_name("PERIOD")
                .help("Interval of time in seconds between messages")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("port")
                .long("port")
                .value_name("PORT")
                .help("Port of the node")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("connect")
                .long("connect")
                .value_name("CONNECT")
                .help("Host and port of another node (e.g. '127.0.0.1:8080')")
                .takes_value(true),
        )
        .get_matches();

    let period = matches.value_of("period").unwrap();
    println!("Value for period: {}", period);

    let port = matches.value_of("port").unwrap();
    println!("Value for port: {}", port);

    let node_address_option = matches.value_of("connect");
    if let Some(node_address) = node_address_option {
        println!("Value for node_address: {}", node_address);
    }

    // TODO: Handle the error.
    let period_u64 = period.parse::<u64>().unwrap();

    run(period_u64, port, node_address_option).await?;

    Ok(())
}

async fn run(
    period: u64,
    port: &str,
    node_address_option: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let server_addr = format!("127.0.0.1:{}", port);
    println!("Listening on {}", server_addr);

    let state = Arc::new(Mutex::new(Shared::new()));

    // Bind the listener to the address.
    let listener = TcpListener::bind(server_addr).await?;

    // Set a timer to "broadcast" messages at a specific interval.
    {
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(period * 1000));

            loop {
                interval.tick().await;

                {
                    let mut state = state.lock().await;

                    if let Err(e) = state.broadcast().await {
                        println!("an error occurred; error = {:?}", e);
                    }
                }
            }
        });
    }

    // Connect to another node if needed.
    if let Some(node_address) = node_address_option {
        let addr = node_address.parse::<SocketAddr>()?;
        println!("trying to connect to: {}", addr);

        let stream = TcpStream::connect(addr).await?;

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = process(state, stream, addr).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }

    // Listen for incoming connections.
    loop {
        let (mut stream, addr) = listener.accept().await?;

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            // Send the list of nodes.
            {
                let state = state.lock().await;
                let peer_addresses = state.get_peer_addresses();

                if peer_addresses.len() != 0 {
                    let mut command = "node-list,".to_string();
                    command.push_str(&peer_addresses);
                    command.push_str("\n");

                    println!("sending command: '{}'", command);

                    if let Err(e) = stream.write_all(command.as_bytes()).await {
                        println!("an error occurred; error = {:?}", e);
                    }
                }
            }

            if let Err(e) = process(state, stream, addr).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

async fn process(
    state: Arc<Mutex<Shared>>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    println!("new peer: {:?}", addr);

    let mut rx;
    {
        let mut state = state.lock().await;
        rx = state.add_peer(addr);
    }

    let mut stream = BufReader::new(stream);
    let mut line = String::new();

    loop {
        tokio::select! {
            Some(cmd) = rx.recv() => {
                use Command::*;

                match cmd {
                    NewTick => {
                        println!("new tick");
                        stream.write_all(b"message,hello world!\n").await?;
                    }
                }
            }
            _ = stream.read_line(&mut line) => {
                println!("received from stream: {}", line);
            }
            else => {
                println!("both channels closed");
                return Ok(())
            }
        }
    }
}
