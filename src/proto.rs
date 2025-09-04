use std::fmt::{self, Debug, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::{Buf, BufMut, Bytes};

#[derive(Clone, Debug, Default)]
pub struct Header {
    pub transaction_id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Qr {
    Request,
    Response,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpCode {
    Query,
    InverseQuery,
    Status,
}

impl Header {
    pub fn qr(&self) -> Qr {
        match self.flags >> 15 {
            0 => Qr::Request,
            1 => Qr::Response,
            _ => unreachable!(),
        }
    }

    pub fn opcode(&self) -> OpCode {
        match (self.flags & 0b0111_1000_0000_0000) >> 11 {
            0 => OpCode::Query,
            1 => OpCode::InverseQuery,
            2 => OpCode::Status,
            //Reserved
            _ => todo!(),
        }
    }

    pub fn aa(&self) -> bool {
        match (self.flags & 0b0000_0100_0000_0000) >> 10 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn tc(&self) -> bool {
        match (self.flags & 0b0000_0010_0000_0000) >> 9 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn rd(&self) -> bool {
        match (self.flags & 0b0000_0001_0000_0000) >> 8 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn ra(&self) -> bool {
        match (self.flags & 0b0000_0000_1000_0000) >> 7 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    pub fn rcode(&self) -> ResponseCode {
        let tag = self.flags & 0b0000_0000_0000_1111;
        ResponseCode::from_u16(tag).unwrap()
    }

    pub fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_u16(self.transaction_id);
        buf.put_u16(self.flags);
        buf.put_u16(self.qdcount);
        buf.put_u16(self.ancount);
        buf.put_u16(self.nscount);
        buf.put_u16(self.arcount);
    }

    pub fn decode<B>(mut buf: B) -> Result<Self, ()>
    where
        B: Buf,
    {
        if buf.remaining() < 12 {
            return Err(());
        }

        Ok(Self {
            transaction_id: buf.get_u16(),
            flags: buf.get_u16(),
            qdcount: buf.get_u16(),
            ancount: buf.get_u16(),
            nscount: buf.get_u16(),
            arcount: buf.get_u16(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub transaction_id: u16,
    pub qr: Qr,
    pub opcode: OpCode,
    pub authoritative_answer: bool,
    pub truncated: bool,
    pub recursion_desired: bool,
    pub recursion_available: bool,
    pub response_code: ResponseCode,
    pub questions: Vec<Question>,
    pub answers: Vec<ResourceRecord>,
    pub authority: Vec<ResourceRecord>,
    pub additional: Vec<ResourceRecord>,
}

impl Packet {
    pub fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(buf);

        if buf.remaining() < 12 {
            return Err(DecodeError::Eof);
        }

        let transaction_id = reader.read_u16().ok_or(DecodeError::Eof)?;
        let flags = reader.read_u16().ok_or(DecodeError::Eof)?;
        let qdcount = reader.read_u16().ok_or(DecodeError::Eof)?;
        let ancount = reader.read_u16().ok_or(DecodeError::Eof)?;
        let nscount = reader.read_u16().ok_or(DecodeError::Eof)?;
        let arcount = reader.read_u16().ok_or(DecodeError::Eof)?;

        let qr = match flags >> 15 {
            0 => Qr::Request,
            1 => Qr::Response,
            _ => unreachable!(),
        };

        let opcode = match (flags & 0b0111_1000_0000_0000) >> 11 {
            0 => OpCode::Query,
            1 => OpCode::InverseQuery,
            2 => OpCode::Status,
            _ => return Err(DecodeError::InvalidOpCode),
        };

        let aa = match (flags & 0b0000_0100_0000_0000) >> 10 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };

        let tc = match (flags & 0b0000_0010_0000_0000) >> 9 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };

        let rd = match (flags & 0b0000_0001_0000_0000) >> 8 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };

        let ra = match (flags & 0b0000_0000_1000_0000) >> 7 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        };

        let rcode = ResponseCode::from_u16(flags & 0b0000_0000_0000_1111)
            .ok_or(DecodeError::InvalidResponseCode)?;

        let mut questions = Vec::new();
        for _ in 0..qdcount {
            questions.push(Question::decode(&mut reader)?);
        }

        let mut answers = Vec::new();
        for _ in 0..ancount {
            answers.push(ResourceRecord::decode(&mut reader)?);
        }

        let mut authority = Vec::new();
        for _ in 0..nscount {
            authority.push(ResourceRecord::decode(&mut reader)?);
        }

        let mut additional = Vec::new();
        for _ in 0..arcount {
            additional.push(ResourceRecord::decode(&mut reader)?);
        }

        Ok(Self {
            transaction_id,
            qr,
            opcode,
            authoritative_answer: aa,
            truncated: tc,
            recursion_desired: rd,
            recursion_available: ra,
            response_code: rcode,
            questions,
            answers,
            additional,
            authority,
        })
    }

    pub fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        let mut flags = 0;
        flags |= match self.qr {
            Qr::Request => 0,
            Qr::Response => 1 << 15,
        };
        flags |= match self.opcode {
            OpCode::Query => 0,
            OpCode::InverseQuery => 1 << 11,
            OpCode::Status => 2 << 11,
        };
        flags |= match self.authoritative_answer {
            false => 0,
            true => 1 << 10,
        };
        flags |= match self.truncated {
            false => 0,
            true => 1 << 9,
        };
        flags |= match self.recursion_desired {
            false => 0,
            true => 1 << 8,
        };
        flags |= match self.recursion_available {
            false => 0,
            true => 1 << 7,
        };
        flags |= self.response_code.to_u16();

        buf.put_u16(self.transaction_id);
        buf.put_u16(flags);
        buf.put_u16(self.questions.len() as u16);
        buf.put_u16(self.answers.len() as u16);
        buf.put_u16(self.authority.len() as u16);
        buf.put_u16(self.additional.len() as u16);

        for question in &self.questions {
            question.encode(&mut buf);
        }

        for resource in &self.answers {
            resource.encode(&mut buf);
        }

        for resource in &self.authority {
            resource.encode(&mut buf);
        }

        for resource in &self.additional {
            resource.encode(&mut buf);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Question {
    pub name: Fqdn,
    pub qtype: Type,
    pub qclass: Class,
}

impl Question {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let name = Fqdn::decode(reader)?;

        let qtype = reader.read_u16().ok_or(DecodeError::Eof)?;
        let qtype = Type::from_bits(qtype);

        let qcalss = reader.read_u16().ok_or(DecodeError::Eof)?;
        let qclass = Class::from_u16(qcalss).ok_or(DecodeError::InvalidClass)?;

        Ok(Self {
            name,
            qclass,
            qtype,
        })
    }

    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        self.name.encode(&mut buf);
        buf.put_u16(self.qtype.to_bits());
        buf.put_u16(self.qclass.to_u16());
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Fqdn(pub Vec<u8>);

impl Fqdn {
    pub fn new_unchecked(fqdn: String) -> Self {
        Self(fqdn.into_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Fqdn {
    fn decode_from_bytes(bytes: &[u8], start: usize) -> Result<(Self, usize), DecodeError> {
        // This implementation will always follow pointers,
        // event if they recursively point to the same pointer.
        // This makes it possible to craft invalid FQDNs that would
        // cause this function to hang forever.
        // To prevent this we process at most `MAX_LABELS` labels
        // and abort if exceeded.
        const MAX_LABELS: usize = 64;

        let mut offset = start;
        let mut advance_count = 0;
        let mut advance_buffer = true;

        let mut labels = Vec::new();
        let mut label_count = 0;

        loop {
            let high = *bytes.get(offset).ok_or(DecodeError::Eof)?;

            if high & 0b1100_0000 != 0 {
                let low = *bytes.get(offset + 1).ok_or(DecodeError::Eof)?;

                if advance_buffer {
                    advance_count += 2;
                }

                advance_buffer = false;

                let pointer = u16::from(high & 0b0011_1111) << 8 | u16::from(low);
                bytes
                    .get(usize::from(pointer)..)
                    .ok_or(DecodeError::BadPointer)?;

                offset = pointer.into();
            }

            let len = *bytes.get(offset).ok_or(DecodeError::Eof)?;
            if len == 0 {
                if advance_buffer {
                    advance_count += 1;
                }

                break;
            }

            let label = bytes
                .get(offset + 1..offset + 1 + usize::from(len))
                .ok_or(DecodeError::Eof)?;

            labels.extend(label);
            labels.push(b'.');

            label_count += 1;
            if label_count == MAX_LABELS {
                return Err(DecodeError::FqdnTooLong);
            }

            offset += usize::from(len) + 1;
            if advance_buffer {
                advance_count += usize::from(len) + 1;
            }
        }

        Ok((Self(labels), advance_count))
    }
}

impl Encode for Fqdn {
    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        for label in self.as_bytes().split(|b| *b == b'.') {
            if label.is_empty() {
                continue;
            }

            buf.put_u8(label.len() as u8);
            buf.put_slice(label);
        }

        buf.put_u8(0);
    }

    fn len(&self) -> u16 {
        self.0.len() as u16 + 1
    }
}

impl Decode for Fqdn {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let bytes = reader.full_buffer();
        let (fqdn, advance_count) = Self::decode_from_bytes(bytes, reader.cursor)?;
        reader.advance(advance_count);

        Ok(fqdn)
    }
}

impl Debug for Fqdn {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Fqdn({:?})",
            std::str::from_utf8(self.as_bytes()).unwrap_or("<invalid utf8>")
        )
    }
}

#[derive(Clone, Debug)]
pub enum RecordData {
    A(Ipv4Addr),
    NS(Fqdn),
    CNAME(Fqdn),
    SOA(SoaData),
    PTR(Fqdn),
    MX(MxData),
    TXT(String),
    AAAA(Ipv6Addr),
    Other(Type, Bytes),
}

impl RecordData {
    fn decode(len: u16, typ: Type, reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let res = match typ {
            Type::A => Ok(Self::A(Ipv4Addr::decode(reader)?)),
            Type::NS => Ok(Self::NS(Fqdn::decode(reader)?)),
            Type::CNAME => Ok(Self::CNAME(Fqdn::decode(reader)?)),
            Type::SOA => Ok(Self::SOA(SoaData::decode(reader)?)),
            Type::PTR => Ok(Self::PTR(Fqdn::decode(reader)?)),
            Type::MX => Ok(Self::MX(MxData::decode(reader)?)),
            Type::AAAA => Ok(Self::AAAA(Ipv6Addr::decode(reader)?)),
            _ => {
                let bytes = reader
                    .remaining_buffer()
                    .get(..usize::from(len))
                    .ok_or(DecodeError::Eof)?
                    .to_vec();

                reader.advance(usize::from(len));

                Ok(Self::Other(typ, Bytes::from(bytes)))
            }
        };

        res
    }

    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        match self {
            Self::A(data) => data.encode(buf),
            Self::NS(data) => data.encode(buf),
            Self::CNAME(data) => data.encode(buf),
            Self::SOA(data) => data.encode(buf),
            Self::PTR(data) => data.encode(buf),
            Self::MX(data) => data.encode(buf),
            Self::TXT(data) => {
                buf.put_slice(data.as_bytes());
            }
            Self::AAAA(data) => data.encode(buf),
            Self::Other(_, data) => {
                buf.put_slice(&data);
            }
        }
    }

    pub fn len(&self) -> u16 {
        match self {
            Self::A(data) => data.len(),
            Self::NS(data) => data.len(),
            Self::CNAME(data) => data.len(),
            Self::SOA(data) => data.len(),
            Self::PTR(data) => data.len(),
            Self::MX(data) => data.len(),
            Self::TXT(data) => data.len() as u16,
            Self::AAAA(data) => data.len(),
            Self::Other(_, data) => data.len() as u16,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Type(u16);

impl Type {
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn to_bits(&self) -> u16 {
        self.0
    }

    /// IPv4 address record.
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const A: Self = Self(1);
    /// Nameserver record.
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const NS: Self = Self(2);
    /// Maildata record.
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const MD: Self = Self(3);
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const CNAME: Self = Self(5);
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const SOA: Self = Self(6);
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const PTR: Self = Self(12);
    /// Mail exchange record.
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const MX: Self = Self(15);
    /// Arbitrary text record.
    ///
    /// Specified in [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035).
    pub const TXT: Self = Self(16);
    // pub const MD: Self = Self(3);
    // pub const MF: Self = Self(4);
    // pub const CNAME: Self = Self(5);
    // pub const SOA: Self = Self(6);
    // pub const MB: Self = Self(7);
    // pub const MINFO: Self = Self(8);
    // pub const MR: Self = Self(9);
    // pub const MX: Self = Self(10);
    // pub const NULL: Self = Self(11);
    pub const OPT: Self = Self(41);

    /// IPv6 address record.
    ///
    /// Specified in [RFC 3596](https://datatracker.ietf.org/doc/html/rfc3596).
    pub const AAAA: Self = Self(28);
}

macro_rules! enum_as_int {
    ($id:ident, $($int:tt => $val:tt),*,) => {
        impl $id {
            pub fn from_u16(tag: u16) -> Option<Self> {
                match tag {
                    $(
                        $int => Some(Self::$val),
                    )*
                    _ => None,
                }
            }

            pub fn to_u16(self) -> u16 {
                match self {
                    $(
                        Self::$val => $int,
                    )*
                }
            }

        }
    };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    In,
}

enum_as_int! {
    Class,
    1 => In,
}

#[derive(Clone, Debug)]
pub struct ResourceRecord {
    pub name: Fqdn,
    pub r#type: Type,
    pub class: Class,
    pub ttl: u32,
    pub rdata: RecordData,
}

impl ResourceRecord {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let name = Fqdn::decode(reader)?;

        let rtype = reader.read_u16().ok_or(DecodeError::Eof)?;
        let r#type = Type::from_bits(rtype);

        // Skip OPT for now
        if r#type == Type::OPT {
            return Ok(Self {
                name,
                r#type,
                ttl: 0,
                class: Class::In,
                rdata: RecordData::Other(Type::OPT, Bytes::new()),
            });
        }

        let class = reader.read_u16().ok_or(DecodeError::Eof)?;
        let class = Class::from_u16(class).ok_or(DecodeError::InvalidClass)?;
        let ttl = reader.read_u32().ok_or(DecodeError::Eof)?;
        let rdlength = reader.read_u16().ok_or(DecodeError::Eof)?;

        let rdata = RecordData::decode(rdlength, r#type, reader)?;

        Ok(Self {
            name,
            r#type,
            class,
            ttl,
            rdata,
        })
    }

    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        self.name.encode(&mut buf);
        buf.put_u16(self.r#type.to_bits());
        buf.put_u16(self.class.to_u16());
        buf.put_u32(self.ttl);
        buf.put_u16(self.rdata.len());
        self.rdata.encode(&mut buf);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResponseCode {
    Ok,
    FormatError,
    ServerFailure,
    NameError,
    NotImplemented,
    Refused,
}

enum_as_int! {
    ResponseCode,
    0 => Ok,
    1 => FormatError,
    2 => ServerFailure,
    3 => NameError,
    4 => NotImplemented,
    5 => Refused,
}

#[derive(Clone, Debug)]
pub enum DecodeError {
    Eof,
    InvalidOpCode,
    InvalidResponseCode,
    InvalidType,
    InvalidClass,
    BadPointer,
    FqdnTooLong,
    UnsupportedType(Type),
    InvalidUtf8,
}

#[derive(Clone, Debug)]
struct Reader<'a> {
    buf: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, cursor: 0 }
    }

    fn read_u8(&mut self) -> Option<u8> {
        let res = self.buf.get(self.cursor).copied();
        self.cursor += 1;
        res
    }

    fn read_u16(&mut self) -> Option<u16> {
        let slice = self.buf.get(self.cursor..self.cursor + 2)?;
        self.cursor += 2;
        Some(u16::from_be_bytes(slice.try_into().unwrap()))
    }

    fn read_u32(&mut self) -> Option<u32> {
        let slice = self.buf.get(self.cursor..self.cursor + 4)?;
        self.cursor += 4;
        Some(u32::from_be_bytes(slice.try_into().unwrap()))
    }

    fn full_buffer(&self) -> &[u8] {
        &self.buf
    }

    fn remaining_buffer(&self) -> &[u8] {
        &self.buf[self.cursor..]
    }

    fn peek_u8(&self) -> Option<u8> {
        self.buf.get(self.cursor).copied()
    }

    fn advance(&mut self, n: usize) {
        self.cursor += n;
    }
}

macro_rules! define_record {
    ($struct_vis:vis struct $struct_name:ident {
        $($field_vis:vis $field_name:ident: $field_type:ty,)*
    }) => {
        #[derive(Clone, Debug)]
        $struct_vis struct $struct_name {
            $(
                $field_vis $field_name: $field_type,
            )*
        }

        impl Encode for $struct_name {
            fn encode<B>(&self, mut buf: B)
            where
                B: BufMut,
            {
                $(
                    self.$field_name.encode(&mut buf);
                )*
            }

            fn len(&self) -> u16 {
                0 $( + self.$field_name.len() )*
            }
        }

        impl Decode for $struct_name {
            fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
                $(
                    let $field_name = <$field_type as Decode>::decode(reader)?;
                )*

                Ok(Self {
                    $(
                        $field_name: $field_name,
                    )*
                })
            }
        }
    };
}

define_record! {
    pub struct SoaData {
        pub mname: Fqdn,
        pub rname: Fqdn,
        pub serial: u32,
        pub refresh: u32,
        pub retry: u32,
        pub expire: u32,
        pub minimum: u32,
    }
}

define_record! {
    pub struct MxData {
        pub preference: u16,
        pub exchange: Fqdn,
    }
}

trait Encode {
    fn encode<B>(&self, buf: B)
    where
        B: BufMut;

    fn len(&self) -> u16;
}

trait Decode: Sized {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError>;
}

impl Encode for u8 {
    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_u8(*self);
    }

    fn len(&self) -> u16 {
        1
    }
}

impl Decode for u8 {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        reader.read_u8().ok_or(DecodeError::Eof)
    }
}

impl Encode for u16 {
    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_u16(*self);
    }

    fn len(&self) -> u16 {
        2
    }
}

impl Decode for u16 {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        reader.read_u16().ok_or(DecodeError::Eof)
    }
}

impl Encode for u32 {
    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_u32(*self);
    }

    fn len(&self) -> u16 {
        4
    }
}

impl Decode for u32 {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        reader.read_u32().ok_or(DecodeError::Eof)
    }
}

impl<const N: usize> Decode for [u8; N] {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut buf = [0; N];
        for index in 0..N {
            buf[index] = u8::decode(reader)?;
        }
        Ok(buf)
    }
}

impl Encode for [u8] {
    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        buf.put_slice(self);
    }

    fn len(&self) -> u16 {
        self.len() as u16
    }
}

impl Encode for Ipv4Addr {
    fn encode<B>(&self, buf: B)
    where
        B: BufMut,
    {
        self.octets().encode(buf);
    }

    fn len(&self) -> u16 {
        self.octets().len() as u16
    }
}

impl Decode for Ipv4Addr {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        <[u8; 4]>::decode(reader).map(Self::from)
    }
}

impl Encode for Ipv6Addr {
    fn encode<B>(&self, buf: B)
    where
        B: BufMut,
    {
        self.octets().encode(buf);
    }

    fn len(&self) -> u16 {
        self.octets().len() as u16
    }
}

impl Decode for Ipv6Addr {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        <[u8; 16]>::decode(reader).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decode, Fqdn, Packet, Reader};

    #[test]
    fn fqdn_decode_basic() {
        let input = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
        ];
        let mut reader = Reader::new(&input);

        let fqdn = Fqdn::decode(&mut reader).unwrap();
        assert_eq!(std::str::from_utf8(&fqdn.0).unwrap(), "example.com.");
    }

    // #[test]
    // fn fqdn_decode_compressed() {
    //     let input = [0b1100_0000, 0b0000_1000];

    //     let mut offset = 0;
    //     let mut labels = HashMap::new();
    //     labels.insert(0, "example.com.".to_owned());
    //     labels.insert(8, "com.".to_owned());

    //     let fqdn = Fqdn::decode(&input[..], &mut offset, &mut labels).unwrap();
    //     assert_eq!(fqdn.0, "com.");
    // }

    #[test]
    fn fqdn_decode_compressed() {
        let mut input = vec![
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
        ];
        let start = input.len();
        input.extend([3, b'w', b'w', b'w', 0b1100_0000, 0b0000_0000]);

        let mut reader = Reader::new(&input);
        reader.advance(start);

        let fqdn = Fqdn::decode(&mut reader).unwrap();
        assert_eq!(std::str::from_utf8(&fqdn.0).unwrap(), "www.example.com.");
    }

    #[test]
    fn fqdn_recursive_offset() {
        let mut input = vec![7, b'e', b'x', b'a', b'm', b'p', b'l', b'e'];
        let start = input.len();
        input.extend([0b1100_0000, 0b0000_0000]);

        let mut reader = Reader::new(&input);
        reader.advance(start);

        Fqdn::decode(&mut reader).unwrap_err();
    }

    #[test]
    fn packet_decode() {
        let payload = [
            0x66, 0xe1, 0x81, 0x80, 0x00, 0x01, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x03, 0x77,
            0x77, 0x77, 0x06, 0x74, 0x77, 0x69, 0x74, 0x63, 0x68, 0x02, 0x74, 0x76, 0x00, 0x00,
            0x01, 0x00, 0x01, 0xc0, 0x0c, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x0d, 0x0f, 0x00,
            0x17, 0x06, 0x74, 0x77, 0x69, 0x74, 0x63, 0x68, 0x03, 0x6d, 0x61, 0x70, 0x06, 0x66,
            0x61, 0x73, 0x74, 0x6c, 0x79, 0x03, 0x6e, 0x65, 0x74, 0x00, 0xc0, 0x2b, 0x00, 0x01,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x04, 0x97, 0x65, 0x02, 0xa7, 0xc0, 0x2b,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x04, 0x97, 0x65, 0xc2, 0xa7,
            0xc0, 0x2b, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x04, 0x97, 0x65,
            0x82, 0xa7, 0xc0, 0x2b, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x04,
            0x97, 0x65, 0x42, 0xa7,
        ];

        let packet = Packet::decode(&payload[..]).unwrap();
    }
}
