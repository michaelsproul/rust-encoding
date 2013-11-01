// This is a part of rust-encoding.
// Copyright (c) 2013, Kang Seonghoon.
// See README.md and LICENSE.txt for details.

//! Common codec implementation for single-byte encodings.

use util::{as_char, StrCharIndex};
use types::*;

pub struct SingleByteEncoding {
    name: &'static str,
    index_forward: extern "Rust" fn(u8) -> u16,
    index_backward: extern "Rust" fn(u16) -> u8,
}

impl Encoding for SingleByteEncoding {
    fn name(&self) -> &'static str { self.name }
    fn encoder(&'static self) -> ~Encoder { ~SingleByteEncoder { encoding: self } as ~Encoder }
    fn decoder(&'static self) -> ~Decoder { ~SingleByteDecoder { encoding: self } as ~Decoder }
}

#[deriving(Clone)]
pub struct SingleByteEncoder {
    encoding: &'static SingleByteEncoding,
}

impl Encoder for SingleByteEncoder {
    fn encoding(&self) -> &'static Encoding { self.encoding as &'static Encoding }

    fn raw_feed(&mut self, input: &str, output: &mut ByteWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        for ((i,j), ch) in input.index_iter() {
            if ch <= '\u007f' {
                output.write_byte(ch as u8);
                loop;
            }
            if ch <= '\uffff' {
                let index = (self.encoding.index_backward)(ch as u16);
                if index != 0xff {
                    output.write_byte((index + 0x80) as u8);
                    loop;
                }
            }
            return (i, Some(CodecError {
                upto: j, cause: "unrepresentable character".into_send_str()
            }));
        }
        (input.len(), None)
    }

    fn raw_finish(&mut self, _output: &mut ByteWriter) -> Option<CodecError> {
        None
    }
}

#[deriving(Clone)]
pub struct SingleByteDecoder {
    encoding: &'static SingleByteEncoding,
}

impl Decoder for SingleByteDecoder {
    fn encoding(&self) -> &'static Encoding { self.encoding as &'static Encoding }

    fn raw_feed(&mut self, input: &[u8], output: &mut StringWriter) -> (uint, Option<CodecError>) {
        output.writer_hint(input.len());

        let mut i = 0;
        let len = input.len();
        while i < len {
            if input[i] <= 0x7f {
                output.write_char(input[i] as char);
            } else {
                let ch = (self.encoding.index_forward)(input[i] - 0x80);
                if ch != 0xffff {
                    output.write_char(as_char(ch));
                } else {
                    return (i, Some(CodecError {
                        upto: i+1, cause: "invalid sequence".into_send_str()
                    }));
                }
            }
            i += 1;
        }
        (i, None)
    }

    fn raw_finish(&mut self, _output: &mut StringWriter) -> Option<CodecError> {
        None
    }
}

#[cfg(test)]
mod tests {
    use all::ISO_8859_2;
    use types::*;

    #[test]
    fn test_encoder_non_bmp() {
        let mut e = ISO_8859_2.encoder();
        assert_feed_err!(e, "A", "\uFFFF", "B", [0x41]);
        assert_feed_err!(e, "A", "\U00010000", "B", [0x41]);
    }
}

