use std::path::PathBuf;
use std::time::Duration;

use notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::{Client, multipart};
use serde::Serialize;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let target_dir = if let Some(target_dir) = std::env::args().skip(1).next() {
        target_dir
    } else {
        panic!("must pass path to watch as first argument");
    };

    let webhook_url = std::env::var("WEBHOOK_URL").expect("must set WEBHOOK_URL env var");

    let (tx, mut rx) = mpsc::channel::<PathBuf>(1);

    tokio::task::spawn_blocking(move || {
        let (tx2, rx) = std::sync::mpsc::channel();
        let mut watcher: RecommendedWatcher = Watcher::new(tx2, Duration::from_secs(2)).expect("failed to create watcher");
        watcher.watch(&target_dir, RecursiveMode::NonRecursive).expect("failed to start watching");
        loop {
            match rx.recv() {
                Ok(event) => {
                    if let DebouncedEvent::Create(path) = event {
                        tx.blocking_send(path).expect("failed to send");
                    }
                }
                Err(e) => eprintln!("watch error: {}", e),
            }
        }
    });

    let client = Client::builder()
        .user_agent("DiscordBot (https://github.com/ashisbored, v1)")
        .build().unwrap();

    while let Some(path) = rx.recv().await {
        let content = tokio::fs::read_to_string(&path).await.expect("failed to read");
        println!("New crash report: {:?}, submitting to Discord...", path);
        let message = WebhookMessage {
            content: "New crash report (attached)".to_string(),
            attachments: vec![Attachment { id: 0 }],
            ..Default::default()
        };
        let form = multipart::Form::new()
            .text("payload_json", serde_json::to_string(&message).unwrap())
            .part("files[0]", multipart::Part::bytes(content.into_bytes())
                .file_name(path.file_name().unwrap().to_string_lossy().to_string()));

        let res = client.post(&webhook_url)
            .multipart(form)
            .send().await;
        match res {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    if let Ok(content) = response.text().await {
                        println!("Got response code {} with body: {}", status, content);
                    } else {
                        println!("Got response code {}", status);
                    }
                }
            },
            Err(e) => eprintln!("Failed to send crash report to Discord: {}", e),
        }
    }
}

#[derive(Serialize, Default)]
struct WebhookMessage {
    content: String,
    allowed_mentions: AllowedMentions,
    attachments: Vec<Attachment>,
}

#[derive(Serialize)]
struct Attachment {
    id: u64,
}

#[derive(Serialize, Default)]
struct AllowedMentions {
    parse: Vec<String>,
}
