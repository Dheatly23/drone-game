use arrayvec::ArrayString;
use boa_engine::prelude::*;
use uuid::Uuid;
use uuid::fmt::Hyphenated;

pub fn js_str_small(s: JsStr<'_>) -> Option<ArrayString<32>> {
    let mut r = ArrayString::<32>::new();

    for c in char::decode_utf16(s.iter()) {
        r.try_push(c.ok()?).ok()?;
    }

    Some(r)
}

pub fn format_uuid(uuid: &Uuid) -> JsString {
    (&*uuid
        .as_hyphenated()
        .encode_lower(&mut [0; Hyphenated::LENGTH]))
        .into()
}
