//! Phase 1 sanity tests for `neoism-terminal-core`.
//!
//! These exist purely to prove the crate is self-contained: they only
//! reach into `neoism_terminal_core::*` plus its declared deps (`copa`).
//! If a future change accidentally drags a native dep back into the
//! crate, the test file failing to compile is the canary.

use copa::{Params, Perform};
use neoism_terminal_core::batch_utf8::{validate_utf8_batch, BatchUtf8Processor};
use neoism_terminal_core::batched_parser::BatchedParser;
use neoism_terminal_core::simd_utf8;

/// Round-trip a UTF-8 byte sequence containing ASCII, a 2-byte char, a
/// 3-byte char, and a 4-byte emoji through the SIMD validator and make
/// sure every code point is preserved.
#[test]
fn simd_utf8_roundtrip() {
    // "Aé—🦀" = U+0041 (1B) + U+00E9 (2B) + U+2014 (3B) + U+1F980 (4B).
    let bytes: &[u8] = b"A\xC3\xA9\xE2\x80\x94\xF0\x9F\xA6\x80";
    let decoded = simd_utf8::from_utf8_fast(bytes).expect("valid utf-8");
    let chars: Vec<char> = decoded.chars().collect();
    assert_eq!(chars, vec!['A', 'é', '—', '🦀']);

    // Lossy path should also produce the same string for valid input.
    assert_eq!(simd_utf8::from_utf8_lossy_fast(bytes), "Aé—🦀");
}

/// Drive a tiny CSI sequence through the batched parser and assert the
/// `csi_dispatch` callback fires with the expected final byte, params,
/// and that the surrounding text bytes print as expected. This proves
/// `BatchedParser` correctly delegates to copa's state machine.
#[test]
fn batched_parser_feeds_csi() {
    #[derive(Default)]
    struct Recorder {
        printed: String,
        csi_finals: Vec<char>,
        csi_params: Vec<Vec<u16>>,
    }

    impl Perform for Recorder {
        fn print(&mut self, c: char) {
            self.printed.push(c);
        }
        fn execute(&mut self, _byte: u8) {}
        fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: char) {}
        fn put(&mut self, _byte: u8) {}
        fn unhook(&mut self) {}
        fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
        fn csi_dispatch(
            &mut self,
            params: &Params,
            _intermediates: &[u8],
            _ignore: bool,
            c: char,
        ) {
            self.csi_finals.push(c);
            let flat: Vec<u16> =
                params.iter().flat_map(|sub| sub.iter().copied()).collect();
            self.csi_params.push(flat);
        }
        fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
    }

    let mut parser = BatchedParser::<1024>::new();
    let mut rec = Recorder::default();

    // ESC [ 1 ; 31 m  hello  ESC [ 0 m
    parser.advance(&mut rec, b"\x1b[1;31mhello\x1b[0m");
    parser.flush(&mut rec);

    assert_eq!(rec.printed, "hello");
    assert_eq!(rec.csi_finals, vec!['m', 'm']);
    assert_eq!(rec.csi_params, vec![vec![1u16, 31], vec![0u16]]);
}

/// `batch_utf8` exists to coalesce many chunks into one SIMD-validated
/// buffer. Verify that:
///   * sub-threshold chunks are *not* batched (caller handles them),
///   * over-threshold chunks accumulate and flush as one valid run,
///   * the processor concatenates chunks so a multibyte char split
///     across two feeds still validates after the rejoin,
///   * the high-level `validate_utf8_batch` uses the joined-buffer path
///     once the total payload crosses its size threshold.
#[test]
fn batch_utf8_chunking() {
    // Processor rejects small chunks below `min_chunk_size`.
    let mut proc = BatchUtf8Processor::new();
    assert!(!proc.try_batch(b"hi"), "tiny chunks should not batch");
    assert!(!proc.has_pending());

    // Large chunks accumulate and flush successfully.
    let big = vec![b'a'; 256];
    assert!(proc.try_batch(&big));
    assert!(proc.has_pending());
    let results = proc.flush_batch();
    assert_eq!(results.len(), 1);
    assert!(results[0].2.is_ok());
    assert_eq!(results[0].1, 256);
    assert!(!proc.has_pending(), "flush should clear buffer");

    // Split a 4-byte emoji ("🦀" = F0 9F A6 80) across two feeds. Each
    // half by itself is *not* valid UTF-8, but the processor stitches
    // them into one buffer before validation, so the joined result
    // must validate cleanly.
    let emoji = "🦀".as_bytes();
    let (head, tail) = emoji.split_at(2);
    // Pad each half so it crosses `min_chunk_size` and gets batched.
    let mut left = vec![b'a'; 64];
    left.extend_from_slice(head);
    let mut right = Vec::with_capacity(64 + tail.len());
    right.extend_from_slice(tail);
    right.extend(std::iter::repeat(b'b').take(64));
    assert!(proc.try_batch(&left));
    assert!(proc.try_batch(&right));
    let results = proc.flush_batch();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].2.is_ok(),
        "multibyte char split across feeds must validate after rejoin"
    );
    assert_eq!(results[0].1, left.len() + right.len());

    // `validate_utf8_batch` switches to the joined-buffer path once the
    // total payload reaches 256 bytes. Build a payload that crosses
    // that threshold and includes a 4-byte emoji to exercise the SIMD
    // path on non-ASCII input.
    let filler = "x".repeat(300);
    let chunks: &[&[u8]] = &[filler.as_bytes(), "🦀".as_bytes()];
    let res = validate_utf8_batch(chunks);
    assert!(res.is_valid);
    assert_eq!(res.chunk_count, 2);
    assert!(res.error_position.is_none());
    assert_eq!(res.bytes_processed, filler.len() + "🦀".len());
}
