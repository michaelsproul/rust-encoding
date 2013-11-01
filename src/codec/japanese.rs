// This is a part of rust-encoding.
// Copyright (c) 2013, Kang Seonghoon.
// See README.md and LICENSE.txt for details.

//! Legacy Japanese encodings based on JIS X 0208 and JIS X 0212.

use util::{as_char, StrCharIndex};
use index0208 = index::jis0208;
use index0212 = index::jis0212;
use types::*;

#[deriving(Clone)]
pub struct EUCJPEncoding;

impl Encoding for EUCJPEncoding {
    fn name(&self) -> &'static str { "euc-jp" }
    fn encoder(&self) -> ~Encoder { ~EUCJPEncoder as ~Encoder }
    fn decoder(&self) -> ~Decoder { ~EUCJPDecoder { first: 0, second: 0 } as ~Decoder }
}

#[deriving(Clone)]
pub struct EUCJPEncoder;

impl Encoder for EUCJPEncoder {
    fn encoding(&self) -> &'static Encoding { &EUCJPEncoding as &'static Encoding }

    fn raw_feed(&mut self, input: &str, output: &mut ByteWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        for ((i,j), ch) in input.index_iter() {
            match ch {
                '\u0000'..'\u007f' => { output.write_byte(ch as u8); }
                '\u00a5' => { output.write_byte(0x5c); }
                '\u203e' => { output.write_byte(0x7e); }
                '\uff61'..'\uff9f' => {
                    output.write_byte(0x8e);
                    output.write_byte((ch as uint - 0xff61 + 0xa1) as u8);
                }
                _ => {
                    let ptr = index0208::backward(ch as u32);
                    if ptr == 0xffff {
                        return (i, Some(CodecError {
                            upto: j, cause: "unrepresentable character".into_send_str()
                        }));
                    } else {
                        let lead = ptr / 94 + 0xa1;
                        let trail = ptr % 94 + 0xa1;
                        output.write_byte(lead as u8);
                        output.write_byte(trail as u8);
                    }
                }
            }
        }
        (input.len(), None)
    }

    fn raw_finish(&mut self, _output: &mut ByteWriter) -> Option<CodecError> {
        None
    }
}

#[deriving(Clone)]
pub struct EUCJPDecoder {
    first: u8,
    second: u8,
}

impl Decoder for EUCJPDecoder {
    fn encoding(&self) -> &'static Encoding { &EUCJPEncoding as &'static Encoding }

    fn raw_feed(&mut self, input: &[u8], output: &mut StringWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        fn map_two_0208_bytes(lead: u8, trail: u8) -> u32 {
            let lead = lead as uint;
            let trail = trail as uint;
            let index = match (lead, trail) {
                (0xa1..0xfe, 0xa1..0xfe) => (lead - 0xa1) * 94 + trail - 0xa1,
                _ => 0xffff,
            };
            index0208::forward(index as u16)
        }

        fn map_two_0212_bytes(lead: u8, trail: u8) -> u32 {
            let lead = lead as uint;
            let trail = trail as uint;
            let index = match (lead, trail) {
                (0xa1..0xfe, 0xa1..0xfe) => (lead - 0xa1) * 94 + trail - 0xa1,
                _ => 0xffff,
            };
            index0212::forward(index as u16)
        }

        let mut i = 0;
        let mut processed = 0;
        let len = input.len();

        if i < len && self.first != 0 {
            match (self.first, input[i]) {
                (0x8e, 0xa1..0xdf) => {
                    output.write_char(as_char(0xff61 + input[i] as uint - 0xa1));
                }
                (0x8f, trail) => {
                    self.first = 0;
                    self.second = trail as u8;
                    // pass through
                }
                (lead, trail) => {
                    let ch = map_two_0208_bytes(lead, trail);
                    if ch == 0xffff {
                        self.first = 0;
                        return (processed, Some(CodecError {
                            upto: i, cause: "invalid sequence".into_send_str()
                        }));
                    }
                    output.write_char(as_char(ch));
                }
            }
            i += 1;
        }

        if i < len && self.second != 0 {
            let ch = map_two_0212_bytes(self.second, input[i]);
            if ch == 0xffff {
                self.second = 0;
                return (processed, Some(CodecError {
                    upto: i, cause: "invalid sequence".into_send_str()
                }));
            }
            output.write_char(as_char(ch));
            i += 1;
        }

        self.first = 0;
        self.second = 0;
        processed = i;
        while i < len {
            match input[i] {
                0x00..0x7f => {
                    output.write_char(input[i] as char);
                }
                0x8e | 0x8f | 0xa1..0xfe => {
                    i += 1;
                    if i >= len {
                        self.first = input[i-1];
                        break;
                    }
                    match (input[i-1], input[i]) {
                        (0x8e, 0xa1..0xdf) => { // JIS X 0201 half-width katakana
                            output.write_char(as_char(0xff61 + input[i] as uint - 0xa1));
                        }
                        (0x8f, 0xa1..0xfe) => { // JIS X 0212 three-byte sequence
                            i += 1;
                            if i >= len {
                                self.second = input[i];
                                break;
                            }
                            let ch = map_two_0212_bytes(input[i-1], input[i]);
                            if ch == 0xffff {
                                return (processed, Some(CodecError {
                                    upto: i, cause: "invalid sequence".into_send_str()
                                }));
                            }
                            output.write_char(as_char(ch));
                        }
                        (0xa1..0xfe, 0xa1..0xfe) => { // JIS X 0208 two-byte sequence
                            let ch = map_two_0208_bytes(input[i-1], input[i]);
                            if ch == 0xffff {
                                return (processed, Some(CodecError {
                                    upto: i, cause: "invalid sequence".into_send_str()
                                }));
                            }
                            output.write_char(as_char(ch));
                        }
                        (_, trail) => {
                            // we should back up when the second byte doesn't look like EUC-JP
                            // (Encoding standard, Chapter 12.1, decoder step 7-4)
                            let upto = if trail < 0xa1 || trail > 0xfe {i} else {i+1};
                            return (processed, Some(CodecError {
                                upto: upto, cause: "invalid sequence".into_send_str()
                            }));
                        }
                    }
                }
                _ => {
                    return (processed, Some(CodecError {
                        upto: i+1, cause: "invalid sequence".into_send_str()
                    }));
                }
            }
            i += 1;
            processed = i;
        }
        (processed, None)
    }

    fn raw_finish(&mut self, _output: &mut StringWriter) -> Option<CodecError> {
        if self.second != 0 || self.first != 0 {
            Some(CodecError { upto: 0, cause: "incomplete sequence".into_send_str() })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod eucjp_tests {
    use super::EUCJPEncoding;
    use types::*;

    #[test]
    fn test_encoder_valid() {
        let mut e = EUCJPEncoding.encoder();
        assert_feed_ok!(e, "A", "", [0x41]);
        assert_feed_ok!(e, "BC", "", [0x42, 0x43]);
        assert_feed_ok!(e, "", "", []);
        assert_feed_ok!(e, "\u00a5", "", [0x5c]);
        assert_feed_ok!(e, "\u203e", "", [0x7e]);
        assert_feed_ok!(e, "\u306b\u307b\u3093", "", [0xa4, 0xcb, 0xa4, 0xdb, 0xa4, 0xf3]);
        assert_feed_ok!(e, "\uff86\uff8e\uff9d", "", [0x8e, 0xc6, 0x8e, 0xce, 0x8e, 0xdd]);
        assert_feed_ok!(e, "\u65e5\u672c", "", [0xc6, 0xfc, 0xcb, 0xdc]);
        assert_finish_ok!(e, []);
    }

    #[test]
    fn test_encoder_invalid() {
        let mut e = EUCJPEncoding.encoder();
        assert_feed_err!(e, "", "\uffff", "", []);
        assert_feed_err!(e, "?", "\uffff", "!", [0x3f]);
        // JIS X 0212 is not supported in the encoder
        assert_feed_err!(e, "", "\u736c", "\u8c78", []);
        assert_finish_ok!(e, []);
    }

    #[test]
    fn test_decoder_valid() {
        let mut d = EUCJPEncoding.decoder();
        assert_feed_ok!(d, [0x41], [], "A");
        assert_feed_ok!(d, [0x42, 0x43], [], "BC");
        assert_feed_ok!(d, [], [], "");
        assert_feed_ok!(d, [0x5c], [], "\\");
        assert_feed_ok!(d, [0x7e], [], "~");
        assert_feed_ok!(d, [0xa4, 0xcb, 0xa4, 0xdb, 0xa4, 0xf3], [], "\u306b\u307b\u3093");
        assert_feed_ok!(d, [0x8e, 0xc6, 0x8e, 0xce, 0x8e, 0xdd], [], "\uff86\uff8e\uff9d");
        assert_feed_ok!(d, [0xc6, 0xfc, 0xcb, 0xdc], [], "\u65e5\u672c");
        assert_feed_ok!(d, [0x8f, 0xcb, 0xc6, 0xec, 0xb8], [], "\u736c\u8c78");
        assert_finish_ok!(d, "");
    }

    // TODO more tests
}

#[deriving(Clone)]
pub struct ShiftJISEncoding;

impl Encoding for ShiftJISEncoding {
    fn name(&self) -> &'static str { "shift-jis" }
    fn encoder(&self) -> ~Encoder { ~ShiftJISEncoder as ~Encoder }
    fn decoder(&self) -> ~Decoder { ~ShiftJISDecoder { lead: 0 } as ~Decoder }
}

#[deriving(Clone)]
pub struct ShiftJISEncoder;

impl Encoder for ShiftJISEncoder {
    fn encoding(&self) -> &'static Encoding { &ShiftJISEncoding as &'static Encoding }

    fn raw_feed(&mut self, input: &str, output: &mut ByteWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        for ((i,j), ch) in input.index_iter() {
            match ch {
                '\u0000'..'\u0080' => { output.write_byte(ch as u8); }
                '\u00a5' => { output.write_byte(0x5c); }
                '\u203e' => { output.write_byte(0x7e); }
                '\uff61'..'\uff9f' => { output.write_byte((ch as uint - 0xff61 + 0xa1) as u8); }
                _ => {
                    let ptr = index0208::backward(ch as u32);
                    if ptr == 0xffff {
                        return (i, Some(CodecError {
                            upto: j, cause: "unrepresentable character".into_send_str(),
                        }));
                    } else {
                        let lead = ptr / 188;
                        let leadoffset = if lead < 0x1f {0x81} else {0xc1};
                        let trail = ptr % 188;
                        let trailoffset = if trail < 0x3f {0x40} else {0x41};
                        output.write_byte((lead + leadoffset) as u8);
                        output.write_byte((trail + trailoffset) as u8);
                    }
                }
            }
        }
        (input.len(), None)
    }

    fn raw_finish(&mut self, _output: &mut ByteWriter) -> Option<CodecError> {
        None
    }
}

#[deriving(Clone)]
pub struct ShiftJISDecoder {
    lead: u8
}

impl Decoder for ShiftJISDecoder {
    fn encoding(&self) -> &'static Encoding { &ShiftJISEncoding as &'static Encoding }

    fn raw_feed(&mut self, input: &[u8], output: &mut StringWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        fn map_two_0208_bytes(lead: u8, trail: u8) -> u32 {
            let lead = lead as uint;
            let trail = trail as uint;
            let index = match (lead, trail) {
                (0x81..0x9f, 0x40..0x7e) | (0x81..0x9f, 0x80..0xfc) |
                (0xe0..0xfc, 0x40..0x7e) | (0xe0..0xfc, 0x80..0xfc) => {
                    let leadoffset = if lead < 0xa0 {0x81} else {0xc1};
                    let trailoffset = if trail < 0x7f {0x40} else {0x41};
                    (lead - leadoffset) * 188 + trail - trailoffset
                }
                _ => 0xffff,
            };
            index0208::forward(index as u16)
        }

        let mut i = 0;
        let mut processed = 0;
        let len = input.len();

        if i < len && self.lead != 0 {
            let ch = map_two_0208_bytes(self.lead, input[i]);
            if ch == 0xffff {
                self.lead = 0;
                return (processed, Some(CodecError {
                    upto: i, cause: "invalid sequence".into_send_str()
                }));
            }
            output.write_char(as_char(ch));
            i += 1;
        }

        self.lead = 0;
        processed = i;
        while i < len {
            match input[i] {
                0x00..0x7f => {
                    output.write_char(input[i] as char);
                }
                0xa1..0xdf => {
                    output.write_char(as_char(0xff61 + (input[i] as uint) - 0xa1));
                }
                0x81..0x9f | 0xe0..0xfc => {
                    i += 1;
                    if i >= len {
                        self.lead = input[i-1];
                        break;
                    }
                    let ch = map_two_0208_bytes(input[i-1], input[i]);
                    if ch == 0xffff {
                        return (processed, Some(CodecError {
                            upto: i, cause: "invalid sequence".into_send_str()
                        }));
                    }
                    output.write_char(as_char(ch));
                }
                _ => {
                    return (processed, Some(CodecError {
                        upto: i+1, cause: "invalid sequence".into_send_str()
                    }));
                }
            }
            i += 1;
            processed = i;
        }
        (processed, None)
    }

    fn raw_finish(&mut self, _output: &mut StringWriter) -> Option<CodecError> {
        if self.lead != 0 {
            Some(CodecError { upto: 0, cause: "incomplete sequence".into_send_str() })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod shiftjis_tests {
    use super::ShiftJISEncoding;
    use types::*;

    #[test]
    fn test_encoder_valid() {
        let mut e = ShiftJISEncoding.encoder();
        assert_feed_ok!(e, "A", "", [0x41]);
        assert_feed_ok!(e, "BC", "", [0x42, 0x43]);
        assert_feed_ok!(e, "", "", []);
        assert_feed_ok!(e, "\u00a5", "", [0x5c]);
        assert_feed_ok!(e, "\u203e", "", [0x7e]);
        assert_feed_ok!(e, "\u306b\u307b\u3093", "", [0x82, 0xc9, 0x82, 0xd9, 0x82, 0xf1]);
        assert_feed_ok!(e, "\uff86\uff8e\uff9d", "", [0xc6, 0xce, 0xdd]);
        assert_feed_ok!(e, "\u65e5\u672c", "", [0x93, 0xfa, 0x96, 0x7b]);
        assert_finish_ok!(e, []);
    }

    #[test]
    fn test_encoder_invalid() {
        let mut e = ShiftJISEncoding.encoder();
        assert_feed_err!(e, "", "\uffff", "", []);
        assert_feed_err!(e, "?", "\uffff", "!", [0x3f]);
        assert_feed_err!(e, "", "\u736c", "\u8c78", []);
        assert_finish_ok!(e, []);
    }

    #[test]
    fn test_decoder_valid() {
        let mut d = ShiftJISEncoding.decoder();
        assert_feed_ok!(d, [0x41], [], "A");
        assert_feed_ok!(d, [0x42, 0x43], [], "BC");
        assert_feed_ok!(d, [], [], "");
        assert_feed_ok!(d, [0x5c], [], "\\");
        assert_feed_ok!(d, [0x7e], [], "~");
        assert_feed_ok!(d, [0x82, 0xc9, 0x82, 0xd9, 0x82, 0xf1], [], "\u306b\u307b\u3093");
        assert_feed_ok!(d, [0xc6, 0xce, 0xdd], [], "\uff86\uff8e\uff9d");
        assert_feed_ok!(d, [0x93, 0xfa, 0x96, 0x7b], [], "\u65e5\u672c");
        assert_finish_ok!(d, "");
    }

    // TODO more tests
}

