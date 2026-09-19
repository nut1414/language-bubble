use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use super::win_space::{WinSpaceDecision, WinSpaceSnapshot};

const TRACE_CAPACITY: usize = 256;
const TRACE_FILE_NAME: &str = "LanguageBubble-win-space-trace.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraceKey {
    LeftWin,
    RightWin,
    Space,
    Control,
}

impl TraceKey {
    const fn label(self) -> &'static str {
        match self {
            Self::LeftWin => "left-win",
            Self::RightWin => "right-win",
            Self::Space => "space",
            Self::Control => "control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraceAction {
    Down,
    Up,
    Other,
}

impl TraceAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraceDisposition {
    Passed,
    Suppressed,
    Neutralized,
    NeutralizationFailed,
    SkippedSelfInjected,
    SkippedSelfSuppression,
}

impl TraceDisposition {
    const fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Suppressed => "suppressed",
            Self::Neutralized => "neutralized",
            Self::NeutralizationFailed => "neutralization-failed",
            Self::SkippedSelfInjected => "self-injected-pass",
            Self::SkippedSelfSuppression => "self-suppression-pass",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TraceInjection {
    pub requested: u8,
    pub sent: u8,
    pub cleanup_requested: u8,
    pub cleanup_sent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WinSpaceTraceRecord {
    pub sequence: u64,
    pub keyboard_time_ms: u32,
    pub key: TraceKey,
    pub action: TraceAction,
    pub flags: u32,
    pub injected: bool,
    pub self_injected: bool,
    pub interception_enabled: bool,
    pub before: WinSpaceSnapshot,
    pub after: WinSpaceSnapshot,
    pub decision: Option<WinSpaceDecision>,
    pub disposition: TraceDisposition,
    pub injection: TraceInjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TraceInput {
    pub keyboard_time_ms: u32,
    pub key: TraceKey,
    pub action: TraceAction,
    pub flags: u32,
    pub injected: bool,
    pub self_injected: bool,
}

impl WinSpaceTraceRecord {
    pub(super) fn new(
        input: TraceInput,
        interception_enabled: bool,
        before: WinSpaceSnapshot,
    ) -> Self {
        Self {
            sequence: 0,
            keyboard_time_ms: input.keyboard_time_ms,
            key: input.key,
            action: input.action,
            flags: input.flags,
            injected: input.injected,
            self_injected: input.self_injected,
            interception_enabled,
            before,
            after: before,
            decision: None,
            disposition: TraceDisposition::Passed,
            injection: TraceInjection::default(),
        }
    }
}

pub(super) struct WinSpaceTraceBuffer {
    records: [Option<WinSpaceTraceRecord>; TRACE_CAPACITY],
    start: usize,
    len: usize,
    next_sequence: u64,
    dropped: u64,
}

impl Default for WinSpaceTraceBuffer {
    fn default() -> Self {
        Self {
            records: [None; TRACE_CAPACITY],
            start: 0,
            len: 0,
            next_sequence: 1,
            dropped: 0,
        }
    }
}

impl WinSpaceTraceBuffer {
    pub(super) fn push(&mut self, mut record: WinSpaceTraceRecord) {
        record.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        if self.len == TRACE_CAPACITY {
            self.records[self.start] = Some(record);
            self.start = (self.start + 1) % TRACE_CAPACITY;
            self.dropped = self.dropped.saturating_add(1);
            return;
        }

        let index = (self.start + self.len) % TRACE_CAPACITY;
        self.records[index] = Some(record);
        self.len += 1;
    }

    pub(super) fn drain(&mut self) -> WinSpaceTraceDrain {
        let mut records = Vec::with_capacity(self.len);
        for offset in 0..self.len {
            let index = (self.start + offset) % TRACE_CAPACITY;
            if let Some(record) = self.records[index].take() {
                records.push(record);
            }
        }

        let dropped = std::mem::take(&mut self.dropped);
        self.start = 0;
        self.len = 0;
        WinSpaceTraceDrain { records, dropped }
    }
}

pub struct WinSpaceTraceDrain {
    records: Vec<WinSpaceTraceRecord>,
    dropped: u64,
}

pub struct WinSpaceTraceFile {
    writer: Option<BufWriter<File>>,
}

impl WinSpaceTraceFile {
    pub fn create() -> Self {
        let path = trace_path();
        let writer = File::create(path).ok().map(BufWriter::new);
        let mut trace = Self { writer };
        trace.write_header();
        trace
    }

    pub fn append(&mut self, drain: WinSpaceTraceDrain) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        let result = (|| -> std::io::Result<()> {
            if drain.dropped != 0 {
                writeln!(writer, "# dropped_records={}", drain.dropped)?;
            }
            for record in drain.records {
                write_record(writer, record)?;
            }
            writer.flush()
        })();

        if result.is_err() {
            self.writer = None;
        }
    }

    fn write_header(&mut self) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let result = writeln!(
            writer,
            "# Language Bubble Win+Space trace version={} arch={} pid={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH,
            std::process::id()
        )
        .and_then(|_| {
            writeln!(
                writer,
                "seq\ttime_ms\tkey\taction\tflags\tinjected\tself_injected\tinterception\tbefore_held\tbefore_space\tbefore_used\tafter_held\tafter_space\tafter_used\tdecision_suppress\tdecision_switch\tdecision_neutralize\tdisposition\tsend_requested\tsend_sent\tcleanup_requested\tcleanup_sent"
            )
        })
        .and_then(|_| writer.flush());

        if result.is_err() {
            self.writer = None;
        }
    }
}

pub fn trace_path() -> PathBuf {
    std::env::temp_dir().join(TRACE_FILE_NAME)
}

fn write_record<W: Write>(writer: &mut W, record: WinSpaceTraceRecord) -> std::io::Result<()> {
    let decision = record.decision.unwrap_or_default();
    writeln!(
        writer,
        "{}\t{}\t{}\t{}\t0x{:X}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        record.sequence,
        record.keyboard_time_ms,
        record.key.label(),
        record.action.label(),
        record.flags,
        u8::from(record.injected),
        u8::from(record.self_injected),
        u8::from(record.interception_enabled),
        record.before.held_win_keys,
        u8::from(record.before.space_suppressed),
        u8::from(record.before.win_used_for_combo),
        record.after.held_win_keys,
        u8::from(record.after.space_suppressed),
        u8::from(record.after.win_used_for_combo),
        u8::from(decision.suppress),
        u8::from(decision.switch_layout),
        u8::from(decision.neutralize_start),
        record.disposition.label(),
        record.injection.requested,
        record.injection.sent,
        record.injection.cleanup_requested,
        record.injection.cleanup_sent,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(time: u32) -> WinSpaceTraceRecord {
        WinSpaceTraceRecord::new(
            TraceInput {
                keyboard_time_ms: time,
                key: TraceKey::Space,
                action: TraceAction::Down,
                flags: 0,
                injected: false,
                self_injected: false,
            },
            true,
            WinSpaceSnapshot::default(),
        )
    }

    #[test]
    fn buffer_preserves_sequence_order() {
        let mut buffer = WinSpaceTraceBuffer::default();
        buffer.push(record(10));
        buffer.push(record(20));

        let drain = buffer.drain();
        assert_eq!(drain.dropped, 0);
        assert_eq!(drain.records.len(), 2);
        assert_eq!(drain.records[0].sequence, 1);
        assert_eq!(drain.records[0].keyboard_time_ms, 10);
        assert_eq!(drain.records[1].sequence, 2);
        assert_eq!(drain.records[1].keyboard_time_ms, 20);
    }

    #[test]
    fn buffer_keeps_latest_records_and_reports_overflow() {
        let mut buffer = WinSpaceTraceBuffer::default();
        for time in 0..(TRACE_CAPACITY as u32 + 3) {
            buffer.push(record(time));
        }

        let drain = buffer.drain();
        assert_eq!(drain.dropped, 3);
        assert_eq!(drain.records.len(), TRACE_CAPACITY);
        assert_eq!(drain.records[0].keyboard_time_ms, 3);
        assert_eq!(
            drain.records.last().unwrap().keyboard_time_ms,
            TRACE_CAPACITY as u32 + 2
        );
    }

    #[test]
    fn serialized_record_contains_only_selected_key_metadata() {
        let mut record = record(42);
        record.flags = 0x10;
        record.injected = true;
        record.self_injected = true;
        record.decision = Some(WinSpaceDecision {
            suppress: true,
            switch_layout: true,
            neutralize_start: false,
        });
        record.disposition = TraceDisposition::Suppressed;
        record.injection = TraceInjection {
            requested: 3,
            sent: 3,
            ..Default::default()
        };

        let mut output = Vec::new();
        write_record(&mut output, record).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("0\t42\tspace\tdown\t0x10"));
        assert!(output.contains("\tsuppressed\t3\t3\t0\t0"));
    }
}
