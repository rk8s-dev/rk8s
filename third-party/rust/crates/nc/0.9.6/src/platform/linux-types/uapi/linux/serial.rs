// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/serial.h`

#![allow(clippy::module_name_repetitions)]

#[repr(C)]
#[derive(Debug, Clone)]
pub struct serial_struct_t {
    pub type_: i32,
    pub line: i32,
    pub port: u32,
    pub irq: i32,
    pub flags: i32,
    pub xmit_fifo_size: i32,
    pub custom_divisor: i32,
    pub baud_base: i32,
    pub close_delay: u16,
    pub io_type: u8,
    pub reserved_char: [u8; 1],
    pub hub6: i32,
    /// time to wait before closing
    pub closing_wait: u16,

    /// no longer used...
    pub closing_wait2: u16,
    pub iomem_base: *mut u8,
    pub iomem_reg_shift: u16,
    pub port_high: u32,

    /// cookie passed into ioremap
    pub iomap_base: usize,
}

/// For the close wait times, 0 means wait forever for serial port to
/// flush its output.  65535 means don't wait at all.
pub const ASYNC_CLOSING_WAIT_INF: i32 = 0;
pub const ASYNC_CLOSING_WAIT_NONE: i32 = 65535;

/// These are the supported serial types.
pub const PORT_UNKNOWN: i32 = 0;
pub const PORT_8250: i32 = 1;
pub const PORT_16450: i32 = 2;
pub const PORT_16550: i32 = 3;
pub const PORT_16550A: i32 = 4;
/// usurped by cyclades.c
pub const PORT_CIRRUS: i32 = 5;
pub const PORT_16650: i32 = 6;
pub const PORT_16650V2: i32 = 7;
pub const PORT_16750: i32 = 8;
/// usurped by cyclades.c
pub const PORT_STARTECH: i32 = 9;
/// Oxford Semiconductor
pub const PORT_16C950: i32 = 10;
pub const PORT_16654: i32 = 11;
pub const PORT_16850: i32 = 12;
/// RSA-DV II/S card
pub const PORT_RSA: i32 = 13;
pub const PORT_MAX: i32 = 13;

pub const SERIAL_IO_PORT: i32 = 0;
pub const SERIAL_IO_HUB6: i32 = 1;
pub const SERIAL_IO_MEM: i32 = 2;
pub const SERIAL_IO_MEM32: i32 = 3;
pub const SERIAL_IO_AU: i32 = 4;
pub const SERIAL_IO_TSI: i32 = 5;
pub const SERIAL_IO_MEM32BE: i32 = 6;
pub const SERIAL_IO_MEM16: i32 = 7;

pub const UART_CLEAR_FIFO: i32 = 0x01;
pub const UART_USE_FIFO: i32 = 0x02;
pub const UART_STARTECH: i32 = 0x04;
pub const UART_NATSEMI: i32 = 0x08;

/// Multiport serial configuration structure --- external structure
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct serial_multiport_struct_t {
    pub irq: i32,
    pub port1: i32,
    pub mask1: u8,
    pub match1: u8,
    pub port2: i32,
    pub mask2: u8,
    pub match2: u8,
    pub port3: i32,
    pub mask3: u8,
    pub match3: u8,
    pub port4: i32,
    pub mask4: u8,
    pub match4: u8,
    pub port_monitor: i32,
    reserved: [i32; 32],
}

/// Serial input interrupt line counters -- external structure
/// Four lines can interrupt: CTS, DSR, RI, DCD
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct serial_icounter_struct_t {
    pub cts: i32,
    pub dsr: i32,
    pub rng: i32,
    pub dcd: i32,

    pub rx: i32,
    pub tx: i32,

    pub frame: i32,
    pub overrun: i32,
    pub parity: i32,
    pub brk: i32,

    pub buf_overrun: i32,
    reserved: [i32; 9],
}

/// Serial interface for controlling RS485 settings on chips with suitable support.
///
/// Set with TIOCSRS485 and get with TIOCGRS485 if supported by your platform.
/// The set function returns the new state, with any unsupported bits reverted appropriately.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct serial_rs485_t {
    /// RS485 feature flags
    pub flags: u32,

    /// Delay before send (milliseconds)
    pub delay_rts_before_send: u32,

    /// Delay after send (milliseconds)
    pub delay_rts_after_send: u32,

    /// Memory is cheap, new structs are a royal PITA ..
    padding: [u32; 5],
}

/// If enabled
pub const SER_RS485_ENABLED: i32 = 1;

/// Logical level for RTS pin when sending
pub const SER_RS485_RTS_ON_SEND: i32 = 1 << 1;

/// Logical level for RTS pin after sent
pub const SER_RS485_RTS_AFTER_SEND: i32 = 1 << 2;

pub const SER_RS485_RX_DURING_TX: i32 = 1 << 4;
/// Enable bus termination (if supported)
pub const SER_RS485_TERMINATE_BUS: i32 = 1 << 5;

/// Serial interface for controlling ISO7816 settings on chips with suitable
/// support. Set with TIOCSISO7816 and get with TIOCGISO7816 if supported by
/// your platform.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct serial_iso7816_t {
    /// ISO7816 feature flags
    pub flags: u32,
    pub tg: u32,
    pub sc_fi: u32,
    pub sc_di: u32,
    pub clk: u32,
    reserved: [u32; 5],
}

pub const SER_ISO7816_ENABLED: i32 = 1;
pub const SER_ISO7816_T_PARAM: i32 = 0x0f << 4;

#[inline]
#[must_use]
pub const fn SER_ISO7816_T(t: i32) -> i32 {
    (t & 0x0f) << 4
}
