// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! AWS Event Stream 流式解码器
//!
//! 使用状态机处理流式数据，支持断点续传和容错处理
//!
//! ## 状态机设计
//!
//! 参考 kiro-kt 项目的状态机设计，采用四态模型：
//!
//! ```text
//! ┌─────────────────┐
//! │      Ready      │  (初始态，就绪接收数据)
//! └────────┬────────┘
//!          │ feed() 提供数据
//!          ↓
//! ┌─────────────────┐
//! │     Parsing     │  decode() 尝试解析
//! └────────┬────────┘
//!          │
//!     ┌────┴────────────┐
//!     ↓                 ↓
//!  [成功]            [失败]
//!     │                 │
//!     ↓                 ├─> error_count++
//! ┌─────────┐           │
//! │  Ready  │           ├─> error_count < max_errors?
//! └─────────┘           │    YES → Recovering → Ready
//!                       │    NO  ↓
//!                  ┌────────────┐
//!                  │   Stopped  │ (终止态)
//!                  └────────────┘
//! ```

use super::error::{ParseError, ParseResult};
use super::frame::{Frame, PRELUDE_SIZE, parse_frame};
use bytes::{Buf, BytesMut};

/// 默认最大缓冲区大小 (16 MB)
pub const DEFAULT_MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// 默认最大连续错误数
pub const DEFAULT_MAX_ERRORS: usize = 5;

/// 默认初始缓冲区容量
pub const DEFAULT_BUFFER_CAPACITY: usize = 8192;

/// 解码器状态
///
/// 采用四态模型，参考 kiro-kt 的设计：
/// - Ready: 就绪状态，可以接收数据
/// - Parsing: 正在解析帧
/// - Recovering: 恢复中（尝试跳过损坏数据）
/// - Stopped: 已停止（错误过多，终止态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderState {
    /// 就绪，可以接收数据
    Ready,
    /// 正在解析帧
    Parsing,
    /// 恢复中（跳过损坏数据）
    Recovering,
    /// 已停止（错误过多）
    Stopped,
}

/// 流式事件解码器
///
/// 用于从字节流中解析 AWS Event Stream 消息帧
///
/// # Example
///
/// ```rust,ignore
/// use kiro_rs::kiro::parser::EventStreamDecoder;
///
/// let mut decoder = EventStreamDecoder::new();
///
/// // 提供流数据
/// decoder.feed(chunk)?;
///
/// // 解码所有可用帧
/// for result in decoder.decode_iter() {
///     match result {
///         Ok(frame) => println!("Got frame: {:?}", frame.event_type()),
///         Err(e) => eprintln!("Parse error: {}", e),
///     }
/// }
/// ```
pub struct EventStreamDecoder {
    /// 内部缓冲区
    buffer: BytesMut,
    /// 当前状态
    state: DecoderState,
    /// 已处理的帧数量
    frames_decoded: usize,
    /// 连续错误计数
    error_count: usize,
    /// 最大连续错误数
    max_errors: usize,
    /// 最大缓冲区大小
    max_buffer_size: usize,
    /// 跳过的字节数（用于调试）
    bytes_skipped: usize,
}

impl Default for EventStreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl EventStreamDecoder {
    /// 创建新的解码器
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUFFER_CAPACITY)
    }

    /// 创建具有指定缓冲区大小的解码器
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
            state: DecoderState::Ready,
            frames_decoded: 0,
            error_count: 0,
            max_errors: DEFAULT_MAX_ERRORS,
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            bytes_skipped: 0,
        }
    }

    /// 创建具有自定义配置的解码器
    pub fn with_config(capacity: usize, max_errors: usize, max_buffer_size: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
            state: DecoderState::Ready,
            frames_decoded: 0,
            error_count: 0,
            max_errors,
            max_buffer_size,
            bytes_skipped: 0,
        }
    }

    /// 向解码器提供数据
    ///
    /// # Returns
    /// - `Ok(())` - 数据已添加到缓冲区
    /// - `Err(BufferOverflow)` - 缓冲区已满
    pub fn feed(&mut self, data: &[u8]) -> ParseResult<()> {
        // 检查缓冲区大小限制
        let new_size = self.buffer.len() + data.len();
        if new_size > self.max_buffer_size {
            return Err(ParseError::BufferOverflow {
                size: new_size,
                max: self.max_buffer_size,
            });
        }

        self.buffer.extend_from_slice(data);

        // 从 Recovering 状态恢复到 Ready
        if self.state == DecoderState::Recovering {
            self.state = DecoderState::Ready;
        }

        Ok(())
    }

    /// 尝试解码下一个帧
    ///
    /// # Returns
    /// - `Ok(Some(frame))` - 成功解码一个帧
    /// - `Ok(None)` - 数据不足，需要更多数据
    /// - `Err(e)` - 解码错误
    pub fn decode(&mut self) -> ParseResult<Option<Frame>> {
        // 如果已停止，直接返回错误
        if self.state == DecoderState::Stopped {
            return Err(ParseError::TooManyErrors {
                count: self.error_count,
                last_error: "解码器已停止".to_string(),
            });
        }

        // 缓冲区为空，保持 Ready 状态
        if self.buffer.is_empty() {
            self.state = DecoderState::Ready;
            return Ok(None);
        }

        // 转移到 Parsing 状态
        self.state = DecoderState::Parsing;

        match parse_frame(&self.buffer) {
            Ok(Some((frame, consumed))) => {
                // 成功解析。契约：consumed == total_length >= MIN_MESSAGE_SIZE(16) > 0
                // （见 frame.rs 的 total_length 下限校验）。consumed > 0 是迭代器
                // 终止的前提——advance(0) 会使 decode_iter 在成功路径上无限吐帧。
                self.buffer.advance(consumed);
                self.state = DecoderState::Ready;
                self.frames_decoded += 1;
                self.error_count = 0; // 重置连续错误计数
                Ok(Some(frame))
            }
            Ok(None) => {
                // 数据不足，回到 Ready 状态等待更多数据
                self.state = DecoderState::Ready;
                Ok(None)
            }
            Err(e) => {
                self.error_count += 1;
                let error_msg = e.to_string();

                // 检查是否超过最大错误数
                if self.error_count >= self.max_errors {
                    self.state = DecoderState::Stopped;
                    tracing::error!(
                        "解码器停止: 连续 {} 次错误，最后错误: {}",
                        self.error_count,
                        error_msg
                    );
                    return Err(ParseError::TooManyErrors {
                        count: self.error_count,
                        last_error: error_msg,
                    });
                }

                // 根据错误类型采用不同的恢复策略
                self.try_recover(&e);
                self.state = DecoderState::Recovering;
                Err(e)
            }
        }
    }

    /// 创建解码迭代器
    pub fn decode_iter(&mut self) -> DecodeIter<'_> {
        DecodeIter { decoder: self }
    }

    /// 尝试容错恢复
    ///
    /// 根据错误类型采用不同的恢复策略（参考 kiro-kt 的设计）：
    /// - Prelude 阶段错误（CRC 失败、长度异常）：跳过 1 字节，尝试找下一帧边界
    /// - Data 阶段错误（Message CRC 失败、Header 解析失败）：跳过整个损坏帧
    fn try_recover(&mut self, error: &ParseError) {
        if self.buffer.is_empty() {
            return;
        }

        match error {
            // Prelude 阶段错误：可能是帧边界错位，逐字节扫描找下一个有效边界
            ParseError::PreludeCrcMismatch { .. }
            | ParseError::MessageTooSmall { .. }
            | ParseError::MessageTooLarge { .. } => {
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "Prelude 错误恢复: 跳过字节 0x{:02x} (累计跳过 {} 字节)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }

            // Data 阶段错误：帧边界正确但数据损坏，跳过整个帧
            ParseError::MessageCrcMismatch { .. } | ParseError::HeaderParseFailed(_) => {
                // 尝试读取 total_length 来跳过整帧
                if self.buffer.len() >= PRELUDE_SIZE {
                    let total_length = u32::from_be_bytes([
                        self.buffer[0],
                        self.buffer[1],
                        self.buffer[2],
                        self.buffer[3],
                    ]) as usize;

                    // 确保 total_length 合理且缓冲区有足够数据
                    if total_length >= 16 && total_length <= self.buffer.len() {
                        tracing::warn!("Data 错误恢复: 跳过损坏帧 ({} 字节)", total_length);
                        self.buffer.advance(total_length);
                        self.bytes_skipped += total_length;
                        return;
                    }
                }

                // 无法确定帧长度，回退到逐字节跳过
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "Data 错误恢复 (回退): 跳过字节 0x{:02x} (累计跳过 {} 字节)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }

            // 其他错误：逐字节跳过
            _ => {
                let skipped_byte = self.buffer[0];
                self.buffer.advance(1);
                self.bytes_skipped += 1;
                tracing::warn!(
                    "通用错误恢复: 跳过字节 0x{:02x} (累计跳过 {} 字节)",
                    skipped_byte,
                    self.bytes_skipped
                );
            }
        }
    }

    // ==================== 生命周期管理方法 ====================

    /// 重置解码器到初始状态
    ///
    /// 清空缓冲区和所有计数器，恢复到 Ready 状态
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.state = DecoderState::Ready;
        self.frames_decoded = 0;
        self.error_count = 0;
        self.bytes_skipped = 0;
    }

    /// 获取当前状态
    pub fn state(&self) -> DecoderState {
        self.state
    }

    /// 检查是否处于 Ready 状态
    pub fn is_ready(&self) -> bool {
        self.state == DecoderState::Ready
    }

    /// 检查是否处于 Stopped 状态
    pub fn is_stopped(&self) -> bool {
        self.state == DecoderState::Stopped
    }

    /// 检查是否处于 Recovering 状态
    pub fn is_recovering(&self) -> bool {
        self.state == DecoderState::Recovering
    }

    /// 获取已解码的帧数量
    pub fn frames_decoded(&self) -> usize {
        self.frames_decoded
    }

    /// 获取当前连续错误计数
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// 获取跳过的字节数
    pub fn bytes_skipped(&self) -> usize {
        self.bytes_skipped
    }

    /// 获取缓冲区中待处理的字节数
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// 尝试从 Stopped 状态恢复
    ///
    /// 重置错误计数并转移到 Ready 状态
    /// 注意：缓冲区内容保留，可能仍包含损坏数据
    pub fn try_resume(&mut self) {
        if self.state == DecoderState::Stopped {
            self.error_count = 0;
            self.state = DecoderState::Ready;
            tracing::info!("解码器从 Stopped 状态恢复");
        }
    }
}

/// 解码迭代器
pub struct DecodeIter<'a> {
    decoder: &'a mut EventStreamDecoder,
}

impl<'a> Iterator for DecodeIter<'a> {
    type Item = ParseResult<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        // 仅 Stopped（错误过多终止态）停止迭代。
        // Recovering 不再提前返回 None：try_recover 已 advance 缓冲区，
        // 应在同一 chunk 内立即重试解析后续帧，否则坏帧之后的有效帧
        // 会被推迟到下个 chunk；若当前是最后一个 chunk，将永久丢失。
        if self.decoder.state == DecoderState::Stopped {
            return None;
        }

        match self.decoder.decode() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_new() {
        let decoder = EventStreamDecoder::new();
        assert_eq!(decoder.state(), DecoderState::Ready);
        assert_eq!(decoder.frames_decoded(), 0);
        assert_eq!(decoder.error_count(), 0);
    }

    #[test]
    fn test_decoder_feed() {
        let mut decoder = EventStreamDecoder::new();
        assert!(decoder.feed(&[1, 2, 3, 4]).is_ok());
        assert_eq!(decoder.buffer_len(), 4);
    }

    #[test]
    fn test_decoder_buffer_overflow() {
        let mut decoder = EventStreamDecoder::with_config(1024, 5, 100);
        let result = decoder.feed(&[0u8; 101]);
        assert!(matches!(result, Err(ParseError::BufferOverflow { .. })));
    }

    #[test]
    fn test_decoder_insufficient_data() {
        let mut decoder = EventStreamDecoder::new();
        decoder.feed(&[0u8; 10]).unwrap();

        let result = decoder.decode();
        assert!(matches!(result, Ok(None)));
        assert_eq!(decoder.state(), DecoderState::Ready);
    }

    #[test]
    fn test_decoder_reset() {
        let mut decoder = EventStreamDecoder::new();
        decoder.feed(&[1, 2, 3, 4]).unwrap();

        decoder.reset();
        assert_eq!(decoder.state(), DecoderState::Ready);
        assert_eq!(decoder.buffer_len(), 0);
        assert_eq!(decoder.frames_decoded(), 0);
    }

    #[test]
    fn test_decoder_state_transitions() {
        let decoder = EventStreamDecoder::new();

        // 初始状态
        assert!(decoder.is_ready());
        assert!(!decoder.is_stopped());
        assert!(!decoder.is_recovering());
    }

    #[test]
    fn test_decoder_try_resume() {
        let mut decoder = EventStreamDecoder::new();

        // 手动设置为 Stopped 状态进行测试
        decoder.state = DecoderState::Stopped;
        decoder.error_count = 5;

        decoder.try_resume();
        assert!(decoder.is_ready());
        assert_eq!(decoder.error_count(), 0);
    }

    /// 构造一个最小合法空帧（total_length=16，无 header 无 payload）
    fn minimal_valid_frame() -> Vec<u8> {
        use crate::kiro::parser::crc::crc32;
        let mut buf = vec![0u8; 16];
        buf[0..4].copy_from_slice(&16u32.to_be_bytes()); // total_length
        buf[4..8].copy_from_slice(&0u32.to_be_bytes()); // header_length
        let prelude_crc = crc32(&buf[0..8]);
        buf[8..12].copy_from_slice(&prelude_crc.to_be_bytes());
        let message_crc = crc32(&buf[0..12]); // 整个消息不含最后 4 字节
        buf[12..16].copy_from_slice(&message_crc.to_be_bytes());
        buf
    }

    /// 回归：坏字节前缀 + 合法帧在同一次 decode_iter 中都应被处理。
    /// 修复前 Recovering 状态会让迭代器提前 return None，导致坏帧之后
    /// 的有效帧被丢弃（若是最后一个 chunk 则永久丢失）。
    #[test]
    fn test_decode_iter_continues_after_recovery() {
        let mut decoder = EventStreamDecoder::new();
        let mut data = vec![0xFFu8]; // 1 个坏字节，触发 Prelude 错误 + 逐字节跳过
        data.extend_from_slice(&minimal_valid_frame());
        decoder.feed(&data).unwrap();

        let mut frames = 0;
        let mut errors = 0;
        for result in decoder.decode_iter() {
            match result {
                Ok(_) => frames += 1,
                Err(_) => errors += 1,
            }
        }

        // 坏字节产生 1 次错误，但合法帧仍在同一次迭代中被解出
        assert_eq!(frames, 1, "坏字节之后的合法帧应在同一 chunk 内被解码");
        assert!(errors >= 1, "坏字节应产生至少一次错误");
        assert_eq!(decoder.frames_decoded(), 1);
    }
}
