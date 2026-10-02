//! Qualified HTTP(S) IP domain canonicalization from .NET 9 Uri helpers.
//! Authority/IRI/scheme parsing has a separate contract in the merge adapter.
use std::{
    fmt::Write,
    net::{Ipv4Addr, Ipv6Addr},
};

pub(super) fn canonical(authority: &str) -> Option<String> {
    if authority.bytes().filter(|&byte| byte == b'@').count() > 1 {
        return None;
    }
    let host_port = authority
        .split_once('@')
        .map_or(authority, |(_, host)| host);
    if let Some(address) = host_port.strip_prefix('[') {
        let (address, port) = address.split_once(']')?;
        if !valid_port(port) {
            return None;
        }
        // Scope identifiers, prefix lengths and nonstandard embedded forms
        // remain outside this strict standard-address intake subset.
        let address: Ipv6Addr = address.parse().ok()?;
        Some(ipv6(address.segments()))
    } else {
        let (address, port) = host_port
            .find(':')
            .map_or((host_port, ""), |index| host_port.split_at(index));
        if !valid_port(port) {
            return None;
        }
        if !address.as_bytes().first()?.is_ascii_digit() {
            return None;
        }
        Some(Ipv4Addr::from(ipv4(address)?).to_string())
    }
}

pub(super) fn valid_port(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let Some(port) = suffix.strip_prefix(':') else {
        return false;
    };
    let mut number = 0u32;
    for byte in port.bytes() {
        if !byte.is_ascii_digit() {
            return false;
        }
        // The accumulator never exceeds 65535, including leading zeroes.
        number = number * 10 + u32::from(byte - b'0');
        if number > 65535 {
            return false;
        }
    }
    true
}

/// IPv4AddressHelper.ParseNonCanonical: prefix bytes plus a combined last part.
fn ipv4(address: &str) -> Option<u32> {
    let mut parts = [0u32; 4];
    let mut count = 0usize;
    for part in address.split('.') {
        if count == parts.len() || part.is_empty() {
            return None;
        }
        let (radix, digits, zero_allowed) =
            if let Some(digits) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
                (16, digits, false)
            } else if let Some(digits) = part.strip_prefix('0') {
                (8, digits, true)
            } else {
                (10, part, false)
            };
        if digits.is_empty() && !zero_allowed {
            return None;
        }
        let mut number = 0u32;
        for byte in digits.bytes() {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => return None,
            };
            if digit >= radix {
                return None;
            }
            number = number.checked_mul(radix)?.checked_add(digit)?;
        }
        parts[count] = number;
        count += 1;
    }
    let last = count.checked_sub(1)?;
    let bits = 32 - last * 8;
    let max = if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    if parts[last] > max {
        return None;
    }
    let mut result = parts[last];
    for (index, &part) in parts[..last].iter().enumerate() {
        if part > 255 {
            return None;
        }
        result |= part << (24 - index * 8);
    }
    Some(result)
}

/// IPv6AddressHelper's longest first zero run and embedded IPv4 formatting.
fn ipv6(numbers: [u16; 8]) -> String {
    let (mut start, mut length, mut current) = (None, 0, 0);
    for (index, &number) in numbers.iter().enumerate() {
        if number == 0 {
            current += 1;
            if current > length {
                length = current;
                start = Some(index + 1 - current);
            }
        } else {
            current = 0;
        }
    }
    if length < 2 {
        start = None;
    }
    let end = start.map_or(0, |start| start + length);
    let embedded = (numbers[..4] == [0; 4]
        && numbers[6] != 0
        && ((numbers[4] == 0 && matches!(numbers[5], 0 | 0xffff))
            || (numbers[4] == 0xffff && numbers[5] == 0)))
        || (numbers[4] == 0 && numbers[5] == 0x5efe);
    let mut out = String::with_capacity(48);
    out.push('[');
    for (index, &number) in numbers.iter().enumerate() {
        // The reference checks embedded output before zero-run compression.
        if embedded && index == 6 {
            write!(
                out,
                ":{}.{}.{}.{}",
                number >> 8,
                number & 255,
                numbers[7] >> 8,
                numbers[7] & 255
            )
            .expect("Writing to String is infallible");
            break;
        }
        if start == Some(index) {
            out.push(':');
        }
        if start.is_some_and(|start| start <= index) && end == numbers.len() {
            out.push(':');
            break;
        }
        if start.is_some_and(|start| start <= index && index < end) {
            continue;
        }
        if index != 0 {
            out.push(':');
        }
        write!(out, "{number:x}").expect("Writing to String is infallible");
    }
    out.push(']');
    out
}
