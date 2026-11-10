//! The file body of a Flow, byte for byte as the Flow tab writes it.
//!
//! The frontend writes `JSON.stringify(doc, null, 2) + "\n"` with the key order
//! of `crate::flow_validation::model` (`src/state/flowDocument/serialize.ts`).
//! Serde's pretty printer writes the same text with two exceptions, which this
//! module removes: serde writes a number in Rust's form (`40.0`, `1e21`), where
//! JavaScript writes `40` and `1e+21`; and the frontend leaves out an empty
//! list, an empty document description, and writes the document name always.
//! So a plan that a sync does not change is written back with the same bytes.

use std::io;

use serde::Serialize;
use serde_json::ser::{Formatter, PrettyFormatter};

use crate::flow_validation::model::FlowDocument;

/// A finite number as ECMAScript `Number::toString` writes it: the shortest
/// digits that read back as the same number, in plain notation from `1e-6` up
/// to below `1e21`, and in exponent notation with a sign outside that range.
pub fn js_number(value: f64) -> String {
    if value == 0.0 {
        // `-0` is written as `0`.
        return "0".to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Rust writes the shortest digits that read back as the same number.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e_sign = if e < 0 { "-" } else { "+" };
        let rest = if k > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{rest}e{e_sign}{}", &digits[..1], e.abs())
    };
    format!("{sign}{body}")
}

/// A pretty printer with a two-space indent that writes numbers as
/// JavaScript does.
struct JsFormatter<'a>(PrettyFormatter<'a>);

impl Formatter for JsFormatter<'_> {
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        if value.is_finite() {
            return writer.write_all(js_number(value).as_bytes());
        }
        self.0.write_f64(writer, value)
    }

    fn begin_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_array(writer)
    }

    fn end_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_array(writer)
    }

    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_array_value(writer, first)
    }

    fn end_array_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_array_value(writer)
    }

    fn begin_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_object(writer)
    }

    fn end_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_object(writer)
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_object_key(writer, first)
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_object_value(writer)
    }

    fn end_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_object_value(writer)
    }
}

/// `doc` with the fields the frontend writes and leaves out: the name always,
/// and no empty description of the document, no empty list of references,
/// artifacts, or footprint paths (`serializeFlowDocument`).
fn as_frontend_writes(doc: &FlowDocument) -> FlowDocument {
    let mut doc = doc.clone();
    doc.name.get_or_insert_with(String::new);
    if doc.description.as_deref() == Some("") {
        doc.description = None;
    }
    if doc.references.as_ref().is_some_and(Vec::is_empty) {
        doc.references = None;
    }
    for item in &mut doc.items {
        if item.artifact_ids.as_ref().is_some_and(Vec::is_empty) {
            item.artifact_ids = None;
        }
        if item.footprint.as_ref().is_some_and(Vec::is_empty) {
            item.footprint = None;
        }
    }
    doc
}

/// The file body of `doc`: two-space JSON and a trailing newline.
pub fn write_flow(doc: &FlowDocument) -> String {
    let doc = as_frontend_writes(doc);
    let mut out = Vec::new();
    let formatter = JsFormatter(PrettyFormatter::with_indent(b"  "));
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    // The model holds only strings, numbers, booleans, lists, and maps with
    // string keys, so serialization cannot fail.
    doc.serialize(&mut serializer).expect("a Flow document always serializes");
    out.push(b'\n');
    String::from_utf8(out).expect("serde_json writes UTF-8")
}
