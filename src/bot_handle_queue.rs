use std::time::Duration;

use crate::minecraft_handle::WebsocketQueue;

pub async fn bot_handle_queue(
    queue: WebsocketQueue,
    bot: azalea::Client,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let command = queue.queue.lock().pop_front();
        let command = match command {
            Some(command) => command,
            None => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let command = command.as_str();

        println!("Recieved command: {}", command);

        match command {
            "sayhi" => {
                bot.chat("hi");
            }
            _ => {
                bot.chat("unknown command");
            }
        };
    }
}
