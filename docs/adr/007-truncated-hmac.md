# 007: Truncated HMAC-SHA256 Frame Authentication

## Context
Standard CAN bus frames have a maximum payload size of 8 bytes. A full cryptographic HMAC-SHA256 signature is 32 bytes (256 bits), which cannot fit inside a standard CAN frame payload. We need a way to authenticate frames without exceeding the payload limits of standard real-time networks.

## Decision
We implement a truncated HMAC-SHA256 scheme. We compute the full HMAC-SHA256 signature and truncate it to the first 8 bytes (64 bits). 

## Consequences
- **Pros**:
  - Signature fits inside standard network frames.
  - A 64-bit signature provides $2^{64}$ security bounds against random collision and forging attacks, which is highly robust against active embedded vehicle spoofing threats.
  - Constant-time verification prevents side-channel timing attacks attempting to brute-force the signature byte-by-byte.
- **Cons**:
  - Reduces cryptographic security from 256 bits to 64 bits. However, in low-bandwidth embedded buses (like CAN @ 500Kbps), brute-forcing $2^{64}$ combinations at runtime is mathematically impossible before the keys rotate or the session expires.
