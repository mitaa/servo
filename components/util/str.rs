/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use app_units::Au;
use libc::c_char;
use num_lib::ToPrimitive;
use std::borrow::ToOwned;
use std::convert::AsRef;
use std::ffi::CStr;
use std::fmt;
use std::iter::{Filter, Peekable};
use std::ops::{Deref, DerefMut};
use std::str::{Bytes, CharIndices, FromStr, Split, from_utf8};

#[derive(Clone, Debug, Deserialize, Eq, Hash, HeapSizeOf, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DOMString(String);

impl !Send for DOMString {}

impl DOMString {
    pub fn new() -> DOMString {
        DOMString(String::new())
    }
    pub fn from_string(s: String) -> DOMString {
        DOMString(s)
    }
    // FIXME(ajeffrey): implement more of the String methods on DOMString?
    pub fn push_str(&mut self, string: &str) {
        self.0.push_str(string)
    }
    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn bytes(&self) -> Bytes {
        self.0.bytes()
    }
}

impl Default for DOMString {
    fn default() -> Self {
        DOMString(String::new())
    }
}

impl Deref for DOMString {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        &self.0
    }
}

impl DerefMut for DOMString {
    #[inline]
    fn deref_mut(&mut self) -> &mut str {
        &mut self.0
    }
}

impl AsRef<str> for DOMString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DOMString {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl PartialEq<str> for DOMString {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}

impl<'a> PartialEq<&'a str> for DOMString {
    fn eq(&self, other: &&'a str) -> bool {
        &**self == *other
    }
}

impl From<String> for DOMString {
    fn from(contents: String) -> DOMString {
        DOMString(contents)
    }
}

impl<'a> From<&'a str> for DOMString {
    fn from(contents: &str) -> DOMString {
        DOMString::from(String::from(contents))
    }
}

impl From<DOMString> for String {
    fn from(contents: DOMString) -> String {
        contents.0
    }
}

impl Into<Vec<u8>> for DOMString {
    fn into(self) -> Vec<u8> {
        self.0.into()
    }
}

impl Extend<char> for DOMString {
    fn extend<I>(&mut self, iterable: I) where I: IntoIterator<Item=char> {
        self.0.extend(iterable)
    }
}

pub type StaticCharVec = &'static [char];
pub type StaticStringVec = &'static [&'static str];

/// Whitespace as defined by HTML5 § 2.4.1.
// TODO(SimonSapin) Maybe a custom Pattern can be more efficient?
pub const WHITESPACE: &'static [char] = &[' ', '\t', '\x0a', '\x0c', '\x0d'];

pub fn is_whitespace(s: &str) -> bool {
    s.chars().all(char_is_whitespace)
}

#[inline]
pub fn char_is_whitespace(c: char) -> bool {
    WHITESPACE.contains(&c)
}

/// A "space character" according to:
///
/// https://html.spec.whatwg.org/multipage/#space-character
pub static HTML_SPACE_CHARACTERS: StaticCharVec = &[
    '\u{0020}',
    '\u{0009}',
    '\u{000a}',
    '\u{000c}',
    '\u{000d}',
];

pub fn split_html_space_chars<'a>(s: &'a str) ->
                                  Filter<Split<'a, StaticCharVec>, fn(&&str) -> bool> {
    fn not_empty(&split: &&str) -> bool { !split.is_empty() }
    s.split(HTML_SPACE_CHARACTERS).filter(not_empty as fn(&&str) -> bool)
}


fn is_ascii_digit(c: &char) -> bool {
    match *c {
        '0'...'9' => true,
        _ => false,
    }
}


pub fn read_numbers<I: Iterator<Item=char>>(mut iter: Peekable<I>) -> Option<i64> {
    match iter.peek() {
        Some(c) if is_ascii_digit(c) => (),
        _ => return None,
    }

    iter.take_while(is_ascii_digit).map(|d| {
        d as i64 - '0' as i64
    }).fold(Some(0i64), |accumulator, d| {
        accumulator.and_then(|accumulator| {
            accumulator.checked_mul(10)
        }).and_then(|accumulator| {
            accumulator.checked_add(d)
        })
    })
}


/// Shared implementation to parse an integer according to
/// <https://html.spec.whatwg.org/multipage/#rules-for-parsing-integers> or
/// <https://html.spec.whatwg.org/multipage/#rules-for-parsing-non-negative-integers>
fn do_parse_integer<T: Iterator<Item=char>>(input: T) -> Option<i64> {
    let mut input = input.skip_while(|c| {
        HTML_SPACE_CHARACTERS.iter().any(|s| s == c)
    }).peekable();

    let sign = match input.peek() {
        None => return None,
        Some(&'-') => {
            input.next();
            -1
        },
        Some(&'+') => {
            input.next();
            1
        },
        Some(_) => 1,
    };

    let value = read_numbers(input);

    value.and_then(|value| value.checked_mul(sign))
}

/// Parse an integer according to
/// <https://html.spec.whatwg.org/multipage/#rules-for-parsing-integers>.
pub fn parse_integer<T: Iterator<Item=char>>(input: T) -> Option<i32> {
    do_parse_integer(input).and_then(|result| {
        result.to_i32()
    })
}

/// Parse an integer according to
/// <https://html.spec.whatwg.org/multipage/#rules-for-parsing-non-negative-integers>
pub fn parse_unsigned_integer<T: Iterator<Item=char>>(input: T) -> Option<u32> {
    do_parse_integer(input).and_then(|result| {
        result.to_u32()
    })
}

#[derive(Clone, Copy, Debug, HeapSizeOf, PartialEq)]
pub enum LengthOrPercentageOrAuto {
    Auto,
    Percentage(f32),
    Length(Au),
}

/// TODO: this function can be rewritten to return Result<LengthOrPercentage, _>
/// Parses a dimension value per HTML5 § 2.4.4.4. If unparseable, `Auto` is
/// returned.
/// https://html.spec.whatwg.org/multipage/#rules-for-parsing-dimension-values
pub fn parse_length(mut value: &str) -> LengthOrPercentageOrAuto {
    // Steps 1 & 2 are not relevant

    // Step 3
    value = value.trim_left_matches(WHITESPACE);

    // Step 4
    if value.is_empty() {
        return LengthOrPercentageOrAuto::Auto
    }

    // Step 5
    if value.starts_with("+") {
        value = &value[1..]
    }

    // Steps 6 & 7
    match value.chars().nth(0) {
        Some('0'...'9') => {},
        _ => return LengthOrPercentageOrAuto::Auto,
    }

    // Steps 8 to 13
    // We trim the string length to the minimum of:
    // 1. the end of the string
    // 2. the first occurence of a '%' (U+0025 PERCENT SIGN)
    // 3. the second occurrence of a '.' (U+002E FULL STOP)
    // 4. the occurrence of a character that is neither a digit nor '%' nor '.'
    // Note: Step 10 is directly subsumed by FromStr::from_str
    let mut end_index = value.len();
    let (mut found_full_stop, mut found_percent) = (false, false);
    for (i, ch) in value.chars().enumerate() {
        match ch {
            '0'...'9' => continue,
            '%' => {
                found_percent = true;
                end_index = i;
                break
            }
            '.' if !found_full_stop => {
                found_full_stop = true;
                continue
            }
            _ => {
                end_index = i;
                break
            }
        }
    }
    value = &value[..end_index];

    if found_percent {
        let result: Result<f32, _> = FromStr::from_str(value);
        match result {
            Ok(number) => return LengthOrPercentageOrAuto::Percentage((number as f32) / 100.0),
            Err(_) => return LengthOrPercentageOrAuto::Auto,
        }
    }

    match FromStr::from_str(value) {
        Ok(number) => LengthOrPercentageOrAuto::Length(Au::from_f64_px(number)),
        Err(_) => LengthOrPercentageOrAuto::Auto,
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize, Serialize)]
pub struct LowercaseString {
    inner: String,
}

impl LowercaseString {
    pub fn new(s: &str) -> LowercaseString {
        LowercaseString {
            inner: s.to_lowercase(),
        }
    }
}

impl Deref for LowercaseString {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        &*self.inner
    }
}

/// Creates a String from the given null-terminated buffer.
/// Panics if the buffer does not contain UTF-8.
pub unsafe fn c_str_to_string(s: *const c_char) -> String {
    from_utf8(CStr::from_ptr(s).to_bytes()).unwrap().to_owned()
}

pub fn str_join<I, T>(strs: I, join: &str) -> String
    where I: IntoIterator<Item=T>, T: AsRef<str>,
{
    strs.into_iter().enumerate().fold(String::new(), |mut acc, (i, s)| {
        if i > 0 { acc.push_str(join); }
        acc.push_str(s.as_ref());
        acc
    })
}

// Lifted from Rust's StrExt implementation, which is being removed.
pub fn slice_chars(s: &str, begin: usize, end: usize) -> &str {
    assert!(begin <= end);
    let mut count = 0;
    let mut begin_byte = None;
    let mut end_byte = None;

    // This could be even more efficient by not decoding,
    // only finding the char boundaries
    for (idx, _) in s.char_indices() {
        if count == begin { begin_byte = Some(idx); }
        if count == end { end_byte = Some(idx); break; }
        count += 1;
    }
    if begin_byte.is_none() && count == begin { begin_byte = Some(s.len()) }
    if end_byte.is_none() && count == end { end_byte = Some(s.len()) }

    match (begin_byte, end_byte) {
        (None, _) => panic!("slice_chars: `begin` is beyond end of string"),
        (_, None) => panic!("slice_chars: `end` is beyond end of string"),
        (Some(a), Some(b)) => unsafe { s.slice_unchecked(a, b) }
    }
}

// searches a character index in CharIndices
// returns indices.count if not found
pub fn search_index(index: usize, indices: CharIndices) -> isize {
    let mut character_count = 0;
    for (character_index, _) in indices {
        if character_index == index {
            return character_count;
        }
        character_count += 1
    }
    character_count
}
