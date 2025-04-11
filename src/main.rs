use zmq;
use chrono::Local;
use std::thread;
use std::time::Duration;

fn log(msg: &str) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    println!("[{}] {}", now, msg);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create ZMQ context and PAIR socket
    let ctx = zmq::Context::new();
    let socket = ctx.socket(zmq::PAIR)?;
    socket.connect("tcp://127.0.0.1:5555")?;
    log("🔌 Connected to MQL5 server");

    // Send subscription request
    let symbol = "BTCUSDT";
    let subscribe_cmd = format!("subscribe:{}", symbol);
    log(&format!("📤 Sending subscription request for {}", symbol));
    socket.send(&subscribe_cmd, 0)?;

    // Wait for subscription confirmation
    match socket.recv_string(0) {
        Ok(Ok(reply)) => log(&format!("🔁 Subscription reply: {}", reply)),
        Ok(Err(e)) => log(&format!("❌ UTF-8 error: {:?}", e)),
        Err(e) => log(&format!("❌ Socket error: {}", e)),
    }

    // Request historical candles (example for M5 timeframe, last 100 candles)
    let history_cmd = format!("history:{}:M5:100", symbol);
    log(&format!("📤 Requesting historical data: {}", history_cmd));
    socket.send(&history_cmd, 0)?;

    // Wait for history data response (this might be large)
    match socket.recv_string(0) {
        Ok(Ok(reply)) => log(&format!("📊 Received historical data (first 100 chars): {}",
                                    if reply.len() > 100 { &reply[0..100] } else { &reply })),
        Ok(Err(e)) => log(&format!("❌ UTF-8 error: {:?}", e)),
        Err(e) => log(&format!("❌ Socket error: {}", e)),
    }

    // Set up polling to handle incoming data
    let mut items = [socket.as_poll_item(zmq::POLLIN)];

    log("🔄 Starting main receive loop");
    loop {
        // Poll with timeout (1 second)
        zmq::poll(&mut items, 1000)?;

        // Check if we have data to receive
        if items[0].is_readable() {
            match socket.recv_string(zmq::DONTWAIT) {
                Ok(Ok(data)) => {
                    // Simply display the raw data
                    log(&format!("📥 Received: {}", data));
                },
                Ok(Err(e)) => log(&format!("❌ UTF-8 error: {:?}", e)),
                Err(e) => log(&format!("❌ Recv error: {}", e)),
            }
        }

        // Small delay to prevent CPU hogging
        thread::sleep(Duration::from_millis(100));
    }
}
//
