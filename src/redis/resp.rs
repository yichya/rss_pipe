use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// RESP 数据类型
#[derive(Debug, Clone)]
pub enum RespValue {
    /// 简单字符串 "+OK\r\n"
    SimpleString(String),
    /// 错误信息 "-Error message\r\n"
    Error(String),
    /// 整数 ":123\r\n"
    Integer(i64),
    /// 批量字符串 "$5\r\nhello\r\n"
    BulkString(String),
    /// 数组 "*2\r\n$5\r\nhello\r\n$5\r\nworld\r\n"
    Array(Vec<RespValue>),
    /// 空值 "$-1\r\n"
    Null,
}

impl RespValue {
    /// 将 RespValue 序列化为 RESP 格式的字节
    pub fn encode(&self) -> Vec<u8> {
        match self {
            RespValue::SimpleString(s) => format!("+{s}\r\n").into_bytes(),
            RespValue::Error(s) => format!("-{s}\r\n").into_bytes(),
            RespValue::Integer(i) => format!(":{i}\r\n").into_bytes(),
            RespValue::BulkString(s) => {
                if s.is_empty() {
                    "$-1\r\n".to_string().into_bytes()
                } else {
                    format!("${}\r\n{}\r\n", s.len(), s).into_bytes()
                }
            }
            RespValue::Null => "$-1\r\n".to_string().into_bytes(),
            RespValue::Array(arr) => {
                let mut result = format!("*{}\r\n", arr.len()).into_bytes();
                for item in arr {
                    result.extend_from_slice(&item.encode());
                }
                result
            }
        }
    }

    /// 创建 OK 响应
    pub fn ok() -> Self {
        RespValue::SimpleString("OK".to_string())
    }

    /// 创建错误响应
    pub fn error(msg: &str) -> Self {
        RespValue::Error(msg.to_string())
    }

    /// 创建整数响应
    pub fn integer(value: i64) -> Self {
        RespValue::Integer(value)
    }

    /// 创建批量字符串响应
    pub fn bulk_string(s: &str) -> Self {
        RespValue::BulkString(s.to_string())
    }

    /// 创建空响应
    pub fn null() -> Self {
        RespValue::Null
    }

    /// 创建数组响应
    pub fn array(values: Vec<RespValue>) -> Self {
        RespValue::Array(values)
    }
}

fn read_value<'a, R: AsyncRead + Unpin + Send + 'a>(
    reader: &'a mut BufReader<R>,
) -> std::pin::Pin<Box<dyn Future<Output = io::Result<RespValue>> + Send + 'a>> {
    Box::pin(async move {
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        if line.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection closed"));
        }

        let line = line.trim_end_matches("\r\n");
        if line.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty line"));
        }

        let prefix = line.as_bytes()[0];
        let data = &line[1..];

        match prefix {
            b'+' => Ok(RespValue::SimpleString(data.to_string())),
            b'-' => Ok(RespValue::Error(data.to_string())),
            b':' => {
                let value = data
                    .parse::<i64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(RespValue::Integer(value))
            }
            b'$' => {
                let len = data
                    .parse::<i64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if len == -1 {
                    return Ok(RespValue::Null);
                }
                if len < 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid bulk string length"));
                }
                let len = len as usize;
                let mut buf = vec![0u8; len + 2]; // +2 for \r\n
                reader.read_exact(&mut buf).await?;
                let s = String::from_utf8(buf[..len].to_vec())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(RespValue::BulkString(s))
            }
            b'*' => {
                let count = data
                    .parse::<i64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if count == -1 {
                    return Ok(RespValue::Null);
                }
                if count < 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid array length"));
                }
                let count = count as usize;
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(read_value(reader).await?);
                }
                Ok(RespValue::Array(values))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown prefix: {}", prefix),
            )),
        }
    })
}

/// 从流中读取并解析 RESP 命令
pub async fn read_command<R: AsyncRead + Unpin + Send>(reader: &mut BufReader<R>) -> io::Result<RespValue> {
    read_value(reader).await
}

/// 将 RespValue 写入流
pub async fn write_response<W: AsyncWrite + Unpin>(writer: &mut W, value: &RespValue) -> io::Result<()> {
    writer.write_all(&value.encode()).await?;
    writer.flush().await?;
    Ok(())
}
