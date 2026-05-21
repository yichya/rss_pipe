use std::sync::OnceLock;
use tokio::io::{BufReader, split};

use crate::common::PrefixedStream;
use crate::common::script::Script;

pub mod commands;
pub mod resp;

static REDIS: OnceLock<commands::RedisCommands> = OnceLock::new();

fn get_commands() -> &'static commands::RedisCommands {
    REDIS.get().expect("Redis not initialized")
}

pub fn init_redis(db_path: &str, set_hook: Option<Script>) {
    REDIS.get_or_init(|| commands::RedisCommands::new(db_path, set_hook));
}

/// 判断是否为 Redis RESP 协议
pub fn is_redis_protocol(first_byte: u8) -> bool {
    first_byte == b'*' || first_byte == b'$' || first_byte == b'+' || first_byte == b'-' || first_byte == b':'
}

/// 处理 Redis 连接
pub async fn handle_connection(prefixed_stream: PrefixedStream) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = split(prefixed_stream);
    let mut buf_reader = BufReader::new(reader);
    let mut selected_db = 0i64;

    loop {
        match resp::read_command(&mut buf_reader).await {
            Ok(command) => {
                let response = get_commands().handle(command, &mut selected_db);
                if let Err(e) = resp::write_response(&mut writer, &response).await {
                    println!("Redis write error: {e}");
                    break;
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                println!("Redis read error: {e}");
                let error_response = resp::RespValue::error(&format!("ERR {e}"));
                let _ = resp::write_response(&mut writer, &error_response).await;
                break;
            }
        }
    }

    Ok(())
}

/// 启动过期键清理任务
pub fn start_cleanup_task(_db_path: &str) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Some(redis) = REDIS.get() {
                match redis.cleanup_expired() {
                    Ok(count) if count > 0 => {
                        println!("Cleaned up {count} expired Redis keys");
                    }
                    Err(e) => {
                        println!("Redis cleanup error: {e}");
                    }
                    _ => {}
                }
            }
        }
    });
}
