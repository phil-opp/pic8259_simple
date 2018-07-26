//! Support for the 8259 Programmable Interrupt Controller, which handles
//! basic I/O interrupts.  In multicore mode, we would apparently need to
//! replace this with an APIC interface.
//!
//! The basic idea here is that we have two PIC chips, PIC1 and PIC2, and
//! that PIC2 is slaved to interrupt 2 on PIC 1.  You can find the whole
//! story at http://wiki.osdev.org/PIC (as usual).  Basically, our
//! immensely sophisticated modern chipset is engaging in early-80s
//! cosplay, and our goal is to do the bare minimum required to get
//! reasonable interrupts.
//!
//! The most important thing we need to do here is set the base "offset"
//! for each of our two PICs, because by default, PIC1 has an offset of
//! 0x8, which means that the I/O interrupts from PIC1 will overlap
//! processor interrupts for things like "General Protection Fault".  Since
//! interrupts 0x00 through 0x1F are reserved by the processor, we move the
//! PIC1 interrupts to 0x20-0x27 and the PIC2 interrupts to 0x28-0x2F.  If
//! we wanted to write a DOS emulator, we'd presumably need to choose
//! different base interrupts, because DOS used interrupt 0x21 for system
//! calls.

#![warn(missing_docs)]
#![feature(const_fn)]
#![no_std]

extern crate x86_64;

use x86_64::instructions::port::Port;

pub const TIMER_INTERRUPT: u8 = PIC_1_OFFSET;

const PIC_1_OFFSET: u8 = 0x20;
const PIC_2_OFFSET: u8 = 0x28;

/// Initialize both our PICs.  We initialize them together, at the same
/// time, because it's traditional to do so, and because I/O operations
/// might not be instantaneous on older processors.
pub unsafe fn initialize() {
    let (mut pic_1, mut pic_2) = create_pic_structs();

    // We need to add a delay between writes to our PICs, especially on
    // older motherboards.  But we don't necessarily have any kind of
    // timers yet, because most of them require interrupts.  Various
    // older versions of Linux and other PC operating systems have
    // worked around this by writing garbage data to port 0x80, which
    // allegedly takes long enough to make everything work on most
    // hardware.  Here, `wait` is a closure.
    let mut wait_port: Port<u8> = Port::new(0x80);
    let mut wait = || wait_port.write(0);

    // Save our original interrupt masks, because I'm too lazy to
    // figure out reasonable values.  We'll restore these when we're
    // done.
    let saved_mask1 = pic_1.data.read();
    let saved_mask2 = pic_2.data.read();

    // Tell each PIC that we're going to send it a three-byte
    // initialization sequence on its data port.
    pic_1.command.write(CMD_INIT);
    wait();
    pic_2.command.write(CMD_INIT);
    wait();

    // Byte 1: Set up our base offsets.
    pic_1.data.write(pic_1.offset);
    wait();
    pic_2.data.write(pic_2.offset);
    wait();

    // Byte 2: Configure chaining between PIC1 and PIC2.
    pic_1.data.write(4);
    wait();
    pic_2.data.write(2);
    wait();

    // Byte 3: Set our mode.
    pic_1.data.write(MODE_8086);
    wait();
    pic_2.data.write(MODE_8086);
    wait();

    // Restore our saved masks.
    pic_1.data.write(saved_mask1);
    pic_2.data.write(saved_mask2);
}

/// Do we handle this interrupt?
pub fn handles_interrupt(interrupt_id: u8) -> bool {
    let (pic_1, pic_2) = create_pic_structs();
    pic_1.handles_interrupt(interrupt_id) || pic_2.handles_interrupt(interrupt_id)
}

/// Figure out which PIC needs to know about this
/// interrupt.  This is tricky, because all interrupts from pic 2
/// get chained through pic 1.
pub unsafe fn notify_end_of_interrupt(interrupt_id: u8) {
    let (mut pic_1, mut pic_2) = create_pic_structs();
    if pic_1.handles_interrupt(interrupt_id) || pic_2.handles_interrupt(interrupt_id) {
        if pic_2.handles_interrupt(interrupt_id) {
            pic_2.end_of_interrupt();
        }
        pic_1.end_of_interrupt();
    }
}

fn create_pic_structs() -> (Pic, Pic) {
    let pic_1 = Pic {
        offset: PIC_1_OFFSET,
        command: Port::new(0x20),
        data: Port::new(0x21),
    };
    let pic_2 = Pic {
        offset: PIC_2_OFFSET,
        command: Port::new(0xA0),
        data: Port::new(0xA1),
    };
    (pic_1, pic_2)
}

/// Command sent to begin PIC initialization.
const CMD_INIT: u8 = 0x11;

/// Command sent to acknowledge an interrupt.
const CMD_END_OF_INTERRUPT: u8 = 0x20;

// The mode in which we want to run our PICs.
const MODE_8086: u8 = 0x01;

/// An individual PIC chip.  This is not exported, because we always access
/// it through `Pics` below.
struct Pic {
    /// The base offset to which our interrupts are mapped.
    offset: u8,

    /// The processor I/O port on which we send commands.
    command: Port<u8>,

    /// The processor I/O port on which we send and receive data.
    data: Port<u8>,
}

impl Pic {
    /// Are we in change of handling the specified interrupt?
    /// (Each PIC handles 8 interrupts.)
    fn handles_interrupt(&self, interrupt_id: u8) -> bool {
        self.offset <= interrupt_id && interrupt_id < self.offset + 8
    }

    /// Notify us that an interrupt has been handled and that we're ready
    /// for more.
    unsafe fn end_of_interrupt(&mut self) {
        self.command.write(CMD_END_OF_INTERRUPT);
    }
}
