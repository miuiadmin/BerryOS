//! berry-nci——G4 刀 15：NCI 协议栈纯面 + ISO 7816 APDU 帧。
//!
//! NCI（NFC Controller Interface）：NCI 控制器消息帧（MT/GID/OID/Payload）。
//! ISO 7816：APDU 命令/应答帧（CLA/INS/P1/P2/Lc/Data/Le + SW1/SW2）。
//!
//! 纯面纪律：帧编解码/校验/状态机 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// NCI 消息类型（Message Type）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NciMessageType {
    Data = 0x00,
    Ntf = 0x40,
    Rsp = 0x80,
    Cmd = 0x20,
}

/// NCI 包头（3B）：MT+PBF+GID | OID | Payload Length。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NciHeader {
    pub message_type: u8,
    pub gid: u8,
    pub oid: u8,
    pub payload_len: u8,
}

/// 编码 NCI 包头 → 3B。
pub fn encode_nci_header(h: &NciHeader) -> [u8; 3] {
    [h.message_type | h.gid, h.oid, h.payload_len]
}

/// 解码 NCI 包头。
pub fn decode_nci_header(b: &[u8]) -> Option<NciHeader> {
    if b.len() < 3 { return None; }
    Some(NciHeader {
        message_type: b[0] & 0xE0,
        gid: b[0] & 0x0F,
        oid: b[1],
        payload_len: b[2],
    })
}

/// ISO 7816-4 APDU 命令（4B 头 + 可选 Lc/Data/Le）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApduCommand {
    pub cla: u8, pub ins: u8, pub p1: u8, pub p2: u8,
    pub data_len: u8,
    /// Lc 数据（最长 255）。
    pub data: [u8; 255],
    /// Le（期望应答长；0 = 无期望）。
    pub le: u8,
}

impl ApduCommand {
    /// 编码为字节流（Case 1=无数据无 Le / Case 2=仅 Le / Case 3=仅数据 / Case 4=双有）。
    pub fn encode(&self) -> [u8; 264] {
        let mut out = [0u8; 264];
        let mut idx = 0usize;
        out[idx] = self.cla; idx += 1; out[idx] = self.ins; idx += 1; out[idx] = self.p1; idx += 1; out[idx] = self.p2; idx += 1;
        if self.data_len > 0 {
            out[idx] = self.data_len; idx += 1;
            for j in 0..self.data_len as usize { out[idx] = self.data[j]; idx += 1; }
        }
        if self.le > 0 {
            out[idx] = self.le; idx += 1;
        }
        let _ = idx;
        out
    }

    /// 从字节流解码 APDU 命令。
    pub fn decode(bytes: &[u8]) -> Option<ApduCommand> {
        if bytes.len() < 4 { return None; }
        let mut cmd = ApduCommand {
            cla: bytes[0], ins: bytes[1], p1: bytes[2], p2: bytes[3],
            data_len: 0, data: [0; 255], le: 0,
        };
        let mut idx = 4;
        if bytes.len() > idx {
            cmd.data_len = bytes[idx];
            idx += 1;
            let end = idx + cmd.data_len as usize;
            if end > bytes.len() { return None; }
            cmd.data[..cmd.data_len as usize].copy_from_slice(&bytes[idx..end]);
            idx = end;
        }
        if bytes.len() > idx {
            cmd.le = bytes[idx];
        }
        Some(cmd)
    }
}

/// APDU 应答（数据 + SW1/SW2 状态字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApduResponse {
    pub sw1: u8, pub sw2: u8,
}

impl ApduResponse {
    /// 成功应答（0x9000）。
    pub const OK: ApduResponse = ApduResponse { sw1: 0x90, sw2: 0x00 };
    /// 是否成功。
    pub fn is_ok(&self) -> bool { self.sw1 == 0x90 && self.sw2 == 0x00 }
}

/// eSE 会话状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EseState {
    Idle,
    Selected,   // AID 已选
    Authenticated, // 已验 PIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nci_header_roundtrip() {
        let h = NciHeader { message_type: 0x00, gid: 0x01, oid: 0x00, payload_len: 0x10 };
        let bytes = encode_nci_header(&h);
        let back = decode_nci_header(&bytes).unwrap();
        assert_eq!(back.gid, 1);
        assert_eq!(back.payload_len, 0x10);
    }

    #[test]
    fn nci_header_too_short() {
        assert!(decode_nci_header(&[0x01]).is_none());
    }

    #[test]
    fn apdu_case1_roundtrip() {
        // Case 1：仅 4B 头（无数据无 Le）。
        let cmd = ApduCommand { cla: 0x00, ins: 0xA4, p1: 0x04, p2: 0x00, data_len: 0, data: [0; 255], le: 0 };
        let bytes = cmd.encode();
        assert_eq!(bytes[0], 0x00); // CLA
        assert_eq!(bytes[1], 0xA4); // INS
        let back = ApduCommand::decode(&bytes).unwrap();
        assert_eq!(back.ins, 0xA4);
    }

    #[test]
    fn apdu_case3_roundtrip() {
        let mut cmd = ApduCommand { cla: 0x00, ins: 0xA4, p1: 0x04, p2: 0x00, data_len: 3, data: [0; 255], le: 0 };
        cmd.data = [0; 255];
        cmd.data[..3].copy_from_slice(&[0xA0, 0x00, 0x00]);
        let bytes = cmd.encode();
        assert_eq!(bytes[4], 3); // Lc
        assert_eq!(bytes[5], 0xA0); // data[0]
        let back = ApduCommand::decode(&bytes).unwrap();
        assert_eq!(back.data_len, 3);
        assert_eq!(&back.data[..3], &[0xA0, 0x00, 0x00]);
    }

    #[test]
    fn apdu_response_ok() {
        assert!(ApduResponse::OK.is_ok());
        assert!(!ApduResponse { sw1: 0x63, sw2: 0x00 }.is_ok());
    }
}
