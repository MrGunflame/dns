use std::collections::HashMap;

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
    pub fn decode<B>(mut buf: B) -> Result<Self, DecodeError>
    where
        B: Buf,
    {
        if buf.remaining() < 12 {
            return Err(DecodeError::Eof);
        }

        let transaction_id = buf.get_u16();
        let flags = buf.get_u16();
        let qdcount = buf.get_u16();
        let ancount = buf.get_u16();
        let nscount = buf.get_u16();
        let arcount = buf.get_u16();

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

        let mut labels = HashMap::new();
        let mut offset = 12;

        let mut questions = Vec::new();
        for _ in 0..qdcount {
            questions.push(Question::decode(&mut buf, &mut offset, &mut labels)?);
        }

        let mut answers = Vec::new();
        for _ in 0..ancount {
            answers.push(ResourceRecord::decode(&mut buf, &mut offset, &mut labels)?);
        }

        let mut authority = Vec::new();
        for _ in 0..nscount {
            authority.push(ResourceRecord::decode(&mut buf, &mut offset, &mut labels)?);
        }

        let mut additional = Vec::new();
        for _ in 0..arcount {
            additional.push(ResourceRecord::decode(&mut buf, &mut offset, &mut labels)?);
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
    fn decode<B>(
        mut buf: B,
        offset: &mut u16,
        labels: &mut HashMap<u16, String>,
    ) -> Result<Self, DecodeError>
    where
        B: Buf,
    {
        let name = Fqdn::decode(&mut buf, offset, labels)?;

        if buf.remaining() < 4 {
            return Err(DecodeError::Eof);
        }
        let qtype = Type::from_u16(buf.get_u16()).ok_or(DecodeError::InvalidType)?;
        let qclass = Class::from_u16(buf.get_u16()).ok_or(DecodeError::InvalidClass)?;
        *offset += 4;
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
        buf.put_u16(self.qtype.to_u16());
        buf.put_u16(self.qclass.to_u16());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fqdn(pub String);

impl Fqdn {
    pub fn new_unchecked(fqdn: String) -> Self {
        Self(fqdn)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Fqdn {
    fn decode<B>(
        mut buf: B,
        offset: &mut u16,
        labels: &mut HashMap<u16, String>,
    ) -> Result<Self, DecodeError>
    where
        B: Buf,
    {
        let offset_start = *offset;

        if buf.remaining() < 1 {
            return Err(DecodeError::Eof);
        }

        // Domain name compression scheme
        let mut len = buf.get_u8();
        *offset += 1;
        if len & 0b1100_0000 != 0 {
            if buf.remaining() < 1 {
                return Err(DecodeError::Eof);
            }

            let pointer = (len as u16 & 0b0011_1111) << 8 | (buf.get_u8() as u16);
            *offset += 1;
            let label = labels.get(&pointer).ok_or(DecodeError::BadPointer)?;

            return Ok(Self(label.clone()));
        }

        let mut fqdn_labels: Vec<String> = Vec::new();

        loop {
            if len == 0 {
                let mut label_offset = offset_start;
                for index in 0..fqdn_labels.len() {
                    let label = fqdn_labels[index..].join("");
                    labels.insert(label_offset, label);
                    label_offset += fqdn_labels[index].len() as u16;
                }

                let mut fqdn = fqdn_labels.join("");

                if fqdn.is_empty() {
                    fqdn.push_str(".");
                }

                return Ok(Self(fqdn));
            }

            let mut label = String::new();

            if buf.remaining() < len.into() {
                return Err(DecodeError::Eof);
            }

            for _ in 0..len {
                let v = buf.get_u8();
                label.push_str(std::str::from_utf8(&[v]).unwrap());
            }
            *offset += len as u16;

            label.push_str(".");
            fqdn_labels.push(label);

            if buf.remaining() < 1 {
                return Err(DecodeError::Eof);
            }
            len = buf.get_u8();
            *offset += 1;
        }
    }

    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        for label in self.0.split('.') {
            if label.is_empty() {
                continue;
            }

            buf.put_u8(label.as_bytes().len() as u8);
            buf.put_slice(label.as_bytes());
        }

        buf.put_u8(0);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    // RFC 1035
    A,
    NS,
    MD,
    MF,
    CNAME,
    SOA,
    MB,
    MG,
    MR,
    NULL,
    WKS,
    PTR,
    HINFO,
    MINFO,
    MX,
    TXT,
    // RFC 3596
    AAAA,
    // RFC 1183
    AFSDB,
    // RFC 3123
    APL,
    // RFC 6844
    CAA,
    // RFC 7344
    CDNSKEY,
    CDS,
    // RFC 4398
    CERT,
    // RFC 7477
    CSYNC,
    // RFC 4701
    DHCID,
    // RFC 4431
    DLV,
    // RFC 6672
    DNAME,
    // RFC 4034
    DNSKEY,
    DS,
    // RFC 7043
    EUI48,
    EUI64,
    // RFC 8005
    HIP,
    // RFC 9460
    HTTPS,
    // RFC 4025
    IPSECKEY,
    // RFC 2535
    KEY,
    // RFC 2230
    KX,
    // RFC 1876
    LOC,
    // RFC 3403
    NAPTR,
    // RFC 4034
    NSEC,
    // RFC 5155
    NSEC3,
    // RFC 5155
    NSEC3PARAM,
    // RFC 7929
    OPENPGPKEY,
    // RFC 4034
    RRSIG,
    // RFC 1183
    RP,
    // RFC 2535
    SIG,
    // RFC 8162
    SMIMEA,
    // RFC 2782
    SRV,
    // RFC 4255
    SSHFP,
    // RFC 9460
    SVCB,
    TA,
    // RFC 2930
    TKEY,
    // RFC 6698
    TLSA,
    // RFC 2845
    TSIG,
    // RFC 7553
    URI,
    // RFC 8976
    ZONEMD,
    /// EDNS
    OPT,
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

enum_as_int! {
    Type,
    1 => A,
    2 => NS,
    3 => MD,
    4 => MF,
    5 => CNAME,
    6 => SOA,
    7 => MB,
    8 => MG,
    9 => MR,
    10 => NULL,
    11 => WKS,
    12 => PTR,
    13 => HINFO,
    14 => MINFO,
    15 => MX,
    16 => TXT,
    28 => AAAA,
    18 => AFSDB,
    42 => APL,
    257 => CAA,
    60 => CDNSKEY,
    59 => CDS,
    37 => CERT,
    62 => CSYNC,
    49 => DHCID,
    32769 => DLV,
    39 => DNAME,
    48 => DNSKEY,
    43 => DS,
    108 => EUI48,
    109 => EUI64,
    55 => HIP,
    65 => HTTPS,
    45 => IPSECKEY,
    25 => KEY,
    36 => KX,
    29 => LOC,
    35 => NAPTR,
    47 => NSEC,
    50 => NSEC3,
    51 => NSEC3PARAM,
    61 => OPENPGPKEY,
    46 => RRSIG,
    17 => RP,
    24 => SIG,
    53 => SMIMEA,
    33 => SRV,
    44 => SSHFP,
    64 => SVCB,
    32768 => TA,
    249 => TKEY,
    52 => TLSA,
    250 => TSIG,
    256 => URI,
    63 => ZONEMD,
    41 => OPT,
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
    pub rddata: Bytes,
}

impl ResourceRecord {
    fn decode<B>(
        mut buf: B,
        offset: &mut u16,
        labels: &mut HashMap<u16, String>,
    ) -> Result<Self, DecodeError>
    where
        B: Buf,
    {
        let name = Fqdn::decode(&mut buf, offset, labels)?;

        if buf.remaining() < 10 {
            return Err(DecodeError::Eof);
        }
        let r#type = Type::from_u16(buf.get_u16()).ok_or(DecodeError::InvalidType)?;
        // Skip OPT for now
        if r#type == Type::OPT {
            return Ok(Self {
                name,
                r#type,
                ttl: 0,
                class: Class::In,
                rddata: Bytes::new(),
            });
        }

        let class = Class::from_u16(buf.get_u16()).ok_or(DecodeError::InvalidClass)?;
        let ttl = buf.get_u32();
        let rdlength = buf.get_u16();
        *offset += 10 + rdlength;

        let mut rddata = Vec::new();
        if buf.remaining() < rdlength.into() {
            return Err(DecodeError::Eof);
        }
        for _ in 0..rdlength {
            rddata.push(buf.get_u8());
        }

        Ok(Self {
            name,
            r#type,
            class,
            ttl,
            rddata: Bytes::from(rddata),
        })
    }

    fn encode<B>(&self, mut buf: B)
    where
        B: BufMut,
    {
        self.name.encode(&mut buf);
        buf.put_u16(self.r#type.to_u16());
        buf.put_u16(self.class.to_u16());
        buf.put_u32(self.ttl);
        buf.put_u16(self.rddata.len() as u16);
        buf.put_slice(&self.rddata);
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::Fqdn;

    #[test]
    fn fqdn_decode_basic() {
        let input = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
        ];
        let mut offset = 0;
        let mut labels = HashMap::new();

        let fqdn = Fqdn::decode(&input[..], &mut offset, &mut labels).unwrap();
        assert_eq!(fqdn.0, "example.com.");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels.get(&0).unwrap(), "example.com.");
        assert_eq!(labels.get(&8).unwrap(), "com.");
    }

    #[test]
    fn fqdn_decode_compressed() {
        let input = [0b1100_0000, 0b0000_1000];

        let mut offset = 0;
        let mut labels = HashMap::new();
        labels.insert(0, "example.com.".to_owned());
        labels.insert(8, "com.".to_owned());

        let fqdn = Fqdn::decode(&input[..], &mut offset, &mut labels).unwrap();
        assert_eq!(fqdn.0, "com.");
    }
}
