use crate::common::script::Script;
use crate::redis::resp::RespValue;
use crate::storage::redis::RedisStorage;

pub struct RedisCommands {
    storage: RedisStorage,
    set_hook: Option<Script>,
}

impl RedisCommands {
    pub fn new(db_path: &str, set_hook: Option<Script>) -> Self {
        Self {
            storage: RedisStorage::new(db_path),
            set_hook,
        }
    }

    pub fn handle(&self, command: RespValue, selected_db: &mut i64) -> RespValue {
        match command {
            RespValue::Array(args) if !args.is_empty() => {
                let cmd_name = match &args[0] {
                    RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
                    _ => return RespValue::error("ERR invalid command format"),
                };

                match cmd_name.as_str() {
                    "SET" => self.cmd_set(*selected_db, &args[1..]),
                    "GET" => self.cmd_get(*selected_db, &args[1..]),
                    "MGET" => self.cmd_mget(*selected_db, &args[1..]),
                    "EXISTS" => self.cmd_exists(*selected_db, &args[1..]),
                    "EXPIRE" => self.cmd_expire(*selected_db, &args[1..]),
                    "SELECT" => self.cmd_select(selected_db, &args[1..]),
                    "PING" => self.cmd_ping(&args[1..]),
                    "INFO" => self.cmd_info(&args[1..]),
                    _ => RespValue::error(&format!("ERR unknown command '{}'", cmd_name)),
                }
            }
            _ => RespValue::error("ERR invalid command format"),
        }
    }

    fn cmd_set(&self, db: i64, args: &[RespValue]) -> RespValue {
        if args.len() < 2 {
            return RespValue::error("ERR wrong number of arguments for 'set' command");
        }

        let key = match &args[0] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
            _ => return RespValue::error("ERR invalid key"),
        };

        let value = match &args[1] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
            _ => return RespValue::error("ERR invalid value"),
        };

        let mut expire_seconds = None;

        if args.len() >= 4 {
            if let Some(RespValue::BulkString(opt) | RespValue::SimpleString(opt)) = args.get(2) {
                if opt.to_uppercase() == "EX" {
                    if let Some(RespValue::BulkString(secs) | RespValue::SimpleString(secs)) = args.get(3) {
                        match secs.parse::<i64>() {
                            Ok(s) if s > 0 => expire_seconds = Some(s),
                            _ => return RespValue::error("ERR invalid expire time"),
                        }
                    }
                }
            }
        }

        let (final_key, final_value, _, final_ttl) = if let Some(hook) = &self.set_hook {
            let hook_result = hook.call_hook("set_hook", (&key, &value, expire_seconds.map(|_| "EX"), expire_seconds));
            hook_result.unwrap_or((key, value, None, expire_seconds))
        } else {
            (key, value, None, expire_seconds)
        };

        match self.storage.set(db, &final_key, &final_value, final_ttl) {
            Ok(()) => RespValue::ok(),
            Err(e) => RespValue::error(&format!("ERR {e}")),
        }
    }

    fn cmd_get(&self, db: i64, args: &[RespValue]) -> RespValue {
        if args.len() != 1 {
            return RespValue::error("ERR wrong number of arguments for 'get' command");
        }

        let key = match &args[0] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
            _ => return RespValue::error("ERR invalid key"),
        };

        match self.storage.get(db, &key) {
            Ok(Some(value)) => RespValue::bulk_string(&value),
            Ok(None) => RespValue::null(),
            Err(e) => RespValue::error(&format!("ERR {e}")),
        }
    }

    fn cmd_mget(&self, db: i64, args: &[RespValue]) -> RespValue {
        if args.is_empty() {
            return RespValue::error("ERR wrong number of arguments for 'mget' command");
        }

        let mut results = Vec::with_capacity(args.len());

        for arg in args {
            let key = match arg {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                _ => {
                    results.push(RespValue::null());
                    continue;
                }
            };

            match self.storage.get(db, &key) {
                Ok(Some(value)) => results.push(RespValue::bulk_string(&value)),
                Ok(None) => results.push(RespValue::null()),
                Err(_) => results.push(RespValue::null()),
            }
        }

        RespValue::array(results)
    }

    fn cmd_exists(&self, db: i64, args: &[RespValue]) -> RespValue {
        if args.is_empty() {
            return RespValue::error("ERR wrong number of arguments for 'exists' command");
        }

        let mut count = 0i64;

        for arg in args {
            let key = match arg {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                _ => continue,
            };

            match self.storage.exists(db, &key) {
                Ok(true) => count += 1,
                Ok(false) => {}
                Err(_) => {}
            }
        }

        RespValue::integer(count)
    }

    fn cmd_expire(&self, db: i64, args: &[RespValue]) -> RespValue {
        if args.len() != 2 {
            return RespValue::error("ERR wrong number of arguments for 'expire' command");
        }

        let key = match &args[0] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
            _ => return RespValue::error("ERR invalid key"),
        };

        let seconds = match &args[1] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => match s.parse() {
                Ok(s) if s > 0 => s,
                _ => return RespValue::error("ERR invalid expire time"),
            },
            RespValue::Integer(i) if *i > 0 => *i,
            _ => return RespValue::error("ERR invalid expire time"),
        };

        match self.storage.expire(db, &key, seconds) {
            Ok(true) => RespValue::integer(1),
            Ok(false) => RespValue::integer(0),
            Err(e) => RespValue::error(&format!("ERR {e}")),
        }
    }

    fn cmd_select(&self, selected_db: &mut i64, args: &[RespValue]) -> RespValue {
        if args.len() != 1 {
            return RespValue::error("ERR wrong number of arguments for 'select' command");
        }

        let db = match &args[0] {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => match s.parse::<i64>() {
                Ok(v) if v >= 0 => v,
                _ => return RespValue::error("ERR invalid DB index"),
            },
            RespValue::Integer(i) if *i >= 0 => *i,
            _ => return RespValue::error("ERR invalid DB index"),
        };

        *selected_db = db;
        RespValue::ok()
    }

    fn cmd_ping(&self, args: &[RespValue]) -> RespValue {
        if args.is_empty() {
            RespValue::SimpleString("PONG".to_string())
        } else {
            match &args[0] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => RespValue::bulk_string(s),
                _ => RespValue::error("ERR invalid message"),
            }
        }
    }

    fn cmd_info(&self, _args: &[RespValue]) -> RespValue {
        let info = "# Server\nredis_version:7.0.0\nredis_mode:standalone\n";
        RespValue::bulk_string(info)
    }

    pub fn cleanup_expired(&self) -> Result<u64, String> {
        self.storage.cleanup_expired()
    }
}
