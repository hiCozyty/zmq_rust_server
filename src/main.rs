use actix::prelude::*;
use actix_web::{web, App, HttpServer, HttpRequest};
use actix_web_actors::ws;
use chrono::Local;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    thread,
};
use zmq;

#[derive(Message)]
#[rtype(result = "()")]
struct ZmqMessage(pub String);

#[derive(Clone)]
struct ZmqBridge {
    socket: Arc<Mutex<zmq::Socket>>,
    clients: Arc<Mutex<HashSet<Addr<MyWs>>>>,
}

impl ZmqBridge {
    fn send(&self, msg: &str) {
        println!("🚀 Sending to ZMQ: {}", msg);

        if let Ok(socket) = self.socket.lock() {
            match socket.send(msg, 0) {  // Using blocking send for reliability
                Ok(_) => {
                    println!("✅ Sent to ZMQ");
                    // Wait for immediate response
                    match socket.recv_string(0) {
                        Ok(Ok(reply)) => println!("📩 Got reply from ZMQ: {}", reply),
                        Ok(Err(e)) => println!("❌ ZMQ UTF-8 decode error: {:?}", e),
                        Err(e) => println!("❌ Socket error: {}", e),
                    }
                },
                Err(e) => eprintln!("❌ Failed to send to ZMQ: {}", e),
            }
        } else {
            eprintln!("❌ Failed to acquire socket lock");
        }
    }
}

struct MyWs {
    bridge: ZmqBridge,
}

impl Actor for MyWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.bridge.clients.lock().unwrap().insert(ctx.address());
        log("🧠 WS client connected");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        self.bridge.clients.lock().unwrap().remove(&ctx.address());
        log("❌ WS client disconnected");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, _ctx: &mut Self::Context) {
        if let Ok(ws::Message::Text(text)) = msg {
            println!("📨 WebSocket msg: {}", text);
            self.bridge.send(&text);
        }
    }
}

impl Handler<ZmqMessage> for MyWs {
    type Result = ();

    fn handle(&mut self, msg: ZmqMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}

fn log(msg: &str) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    println!("[{}] {}", now, msg);
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let ctx = zmq::Context::new();
    let socket = ctx.socket(zmq::PAIR).unwrap();

    // Configure socket for real-time performance
    socket.set_linger(0).unwrap();
    socket.set_immediate(true).unwrap();
    socket.set_sndhwm(1000).unwrap();
    socket.set_rcvhwm(1000).unwrap();

    socket.connect("tcp://127.0.0.1:5555").unwrap();
    log("🔌 Connected to MQL5 via ZMQ");

    let bridge = ZmqBridge {
        socket: Arc::new(Mutex::new(socket)),
        clients: Arc::new(Mutex::new(HashSet::new())),
    };

    // ZMQ receive thread
    let bridge_clone = bridge.clone();
    thread::spawn(move || {
        let socket = bridge_clone.socket.clone();
        let clients = bridge_clone.clients.clone();

        loop {
            if let Ok(socket_guard) = socket.lock() {
                // Direct blocking receive
                match socket_guard.recv_string(0) {
                    Ok(Ok(msg)) => {
                        drop(socket_guard); // Release lock before client operations
                        log(&format!("📊 ZMQ -> WS: {}", msg));

                        if let Ok(clients_guard) = clients.lock() {
                            for client in clients_guard.iter() {
                                client.do_send(ZmqMessage(msg.clone()));
                            }
                        }
                    },
                    Ok(Err(e)) => log(&format!("❌ UTF-8 error: {:?}", e)),
                    Err(e) => log(&format!("❌ Socket error: {}", e)),
                }
            }
        }
    });

    let mut ssl_builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    ssl_builder
        .set_private_key_file("fedora.tail8a383a.ts.net.key", SslFiletype::PEM)
        .unwrap();
    ssl_builder
        .set_certificate_chain_file("fedora.tail8a383a.ts.net.crt")
        .unwrap();

    HttpServer::new(move || {
        App::new().route("/ws", web::get().to({
            let bridge = bridge.clone();
            move |req: HttpRequest, stream: web::Payload| {
                let ws = MyWs { bridge: bridge.clone() };
                async move { ws::start(ws, &req, stream) }
            }
        }))
    })
    .bind_openssl("0.0.0.0:8443", ssl_builder)?
    .run()
    .await
}
