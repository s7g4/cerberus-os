//! CAN Bus protocol stack and SPSC ring buffer.

pub const CAN_MAX_PAYLOAD: usize = 8;
pub const RING_CAPACITY: usize = 16; // Must be power of 2 for mask index optimization

/// Standard CAN 2.0A frame representation.
#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct CanFrame {
    /// 11-bit standard CAN identifier.
    pub id: u16,
    /// Data Length Code (number of payload bytes, 0-8).
    pub dlc: u8,
    /// Payload buffer.
    pub payload: [u8; CAN_MAX_PAYLOAD],
}

/// CAN subsystem errors.
#[derive(Debug, Clone, Copy, defmt::Format)]
pub enum CanError {
    /// Received a Frame ID that is blocked by the security filter.
    BlockedId(u16),
    /// Invalid Data Length Code (DLC > 8).
    InvalidDlc(u8),
    /// Ring buffer queue is full.
    BufferFull,
}

/// Diagnostic override frames to reject.
/// 0x7DF: OBD-II broadcast request (common entry point for replay attacks).
/// 0x7E0-0x7EF: ECU diagnostic query/response addresses.
const BLOCKED_IDS: &[u16] = &[0x7DF, 0x7E0, 0x7E1, 0x7E8];

impl CanFrame {
    /// Parses a raw 13-byte transceiver frame.
    ///
    /// Layout:
    /// Byte 0: ID bits [10:3] (MSB)
    /// Byte 1: ID bits [2:0] (LSB) in top 3 bits, followed by RTR and control bits
    /// Byte 2: DLC (data length) in lower 4 bits
    /// Bytes 3-10: Payload data (up to 8 bytes)
    pub fn parse(raw: &[u8; 13]) -> Result<Self, CanError> {
        // Extract 11-bit standard identifier
        let id = ((raw[0] as u16) << 3) | ((raw[1] as u16) >> 5);

        // Security boundary: filter out diagnostic IDs to prevent spoofing
        if BLOCKED_IDS.contains(&id) {
            return Err(CanError::BlockedId(id));
        }

        let dlc = raw[2] & 0x0F;
        if dlc > 8 {
            return Err(CanError::InvalidDlc(dlc));
        }

        let mut payload = [0u8; CAN_MAX_PAYLOAD];
        payload[..dlc as usize].copy_from_slice(&raw[3..3 + dlc as usize]);

        Ok(CanFrame { id, dlc, payload })
    }
}

/// Lock-free Single-Producer Single-Consumer (SPSC) Ring Buffer.
pub struct CanRingBuffer {
    frames: [Option<CanFrame>; RING_CAPACITY],
    head: usize,
    tail: usize,
}

impl Default for CanRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CanRingBuffer {
    pub const fn new() -> Self {
        // Compile-time assert that capacity is a power of two
        const _: () = assert!(
            RING_CAPACITY.is_power_of_two(),
            "Ring buffer capacity must be a power of 2 for mask optimization"
        );
        Self {
            frames: [None; RING_CAPACITY],
            head: 0,
            tail: 0,
        }
    }

    /// Push a frame to the tail of the buffer.
    pub fn push(&mut self, frame: CanFrame) -> Result<(), CanError> {
        if self.is_full() {
            return Err(CanError::BufferFull);
        }
        self.frames[self.tail] = Some(frame);
        // Fast index masking instead of division modulo: (tail + 1) & (CAPACITY - 1)
        self.tail = (self.tail + 1) & (RING_CAPACITY - 1);
        Ok(())
    }

    /// Pop a frame from the head of the buffer.
    pub fn pop(&mut self) -> Option<CanFrame> {
        if self.is_empty() {
            None
        } else {
            let frame = self.frames[self.head].take();
            self.head = (self.head + 1) & (RING_CAPACITY - 1);
            frame
        }
    }

    pub fn is_full(&self) -> bool {
        ((self.tail + 1) & (RING_CAPACITY - 1)) == self.head
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_frame(id: u16, dlc: u8, payload: &[u8]) -> [u8; 13] {
        let mut raw = [0u8; 13];
        raw[0] = (id >> 3) as u8;
        raw[1] = ((id & 0x7) as u8) << 5;
        raw[2] = dlc;
        raw[3..3 + payload.len()].copy_from_slice(payload);
        raw
    }

    #[test]
    fn parse_extracts_id_dlc_and_payload() {
        let raw = raw_frame(0x123, 4, &[0xAA, 0xBB, 0xCC, 0xDD]);
        let frame = CanFrame::parse(&raw).unwrap();
        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 4);
        assert_eq!(&frame.payload[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn parse_rejects_blocked_diagnostic_ids() {
        let raw = raw_frame(0x7DF, 0, &[]);
        assert!(matches!(
            CanFrame::parse(&raw),
            Err(CanError::BlockedId(0x7DF))
        ));
    }

    #[test]
    fn parse_rejects_dlc_over_eight() {
        let mut raw = raw_frame(0x100, 0, &[]);
        raw[2] = 9;
        assert!(matches!(
            CanFrame::parse(&raw),
            Err(CanError::InvalidDlc(9))
        ));
    }

    #[test]
    fn ring_buffer_push_pop_preserves_order() {
        let mut ring = CanRingBuffer::new();
        assert!(ring.is_empty());
        for i in 0..4u16 {
            ring.push(CanFrame {
                id: i,
                dlc: 0,
                payload: [0u8; CAN_MAX_PAYLOAD],
            })
            .unwrap();
        }
        for i in 0..4u16 {
            assert_eq!(ring.pop().unwrap().id, i);
        }
        assert!(ring.is_empty());
    }

    #[test]
    fn ring_buffer_rejects_push_when_full() {
        let mut ring = CanRingBuffer::new();
        // Capacity is RING_CAPACITY - 1 usable slots (one slot always kept empty
        // to distinguish full from empty using head == tail).
        for i in 0..(RING_CAPACITY - 1) as u16 {
            ring.push(CanFrame {
                id: i,
                dlc: 0,
                payload: [0u8; CAN_MAX_PAYLOAD],
            })
            .unwrap();
        }
        assert!(ring.is_full());
        let overflow = ring.push(CanFrame {
            id: 999,
            dlc: 0,
            payload: [0u8; CAN_MAX_PAYLOAD],
        });
        assert!(matches!(overflow, Err(CanError::BufferFull)));
    }
}
