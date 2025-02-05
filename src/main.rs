#![allow(static_mut_refs)] // TODO: We should avoid using these altogether
#![no_main]
#![no_std]

use flasher_stub::{
    dprintln,
    hal::{
        self,
        clock::CpuClock,
        gpio::NoPin,
        main,
        peripherals,
        uart::{ClockSource, Config, Uart, UartInterrupt},
        Blocking,
    },
    io::uart::uart0_handler,
    protocol::Stub,
    targets,
    Transport,
    TransportMethod,
};
use static_cell::StaticCell;

const MSG_BUFFER_SIZE: usize = targets::MAX_WRITE_BLOCK + 0x400;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    dprintln!("STUB Panic: {:?}", _info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = hal::init(hal::Config::default().with_cpu_clock(CpuClock::max()));

    // If the `dprint` feature is enabled, configure/initialize the debug console,
    // which prints via UART1:

    #[cfg(feature = "dprint")]
    let _ = Uart::new(peripherals.UART1, peripherals.GPIO0, peripherals.GPIO2);

    // Detect the transport method being used, and configure/initialize the
    // corresponding peripheral:

    let transport = TransportMethod::detect();
    dprintln!("Stub init! Transport detected: {:?}", transport);

    let transport = match transport {
        TransportMethod::Uart => transport_uart(peripherals.UART0),
        #[cfg(usb_device)]
        TransportMethod::UsbSerialJtag => transport_usb_serial_jtag(peripherals.USB_DEVICE),
        #[cfg(usb0)]
        TransportMethod::UsbOtg => unimplemented!(),
    };

    // With the transport initialized we can move on to initializing the stub
    // itself:

    let mut stub = Stub::new(transport);
    dprintln!("Stub sending greeting!");
    stub.send_greeting();

    // With the stub initialized and the greeting sent, all that's left to do is to
    // wait for commands to process:

    let mut buffer: [u8; MSG_BUFFER_SIZE] = [0; MSG_BUFFER_SIZE];
    loop {
        dprintln!("Waiting for command");
        let data = stub.read_command(&mut buffer);
        dprintln!("Processing command");
        stub.process_command(data);
    }
}

// Initialize the UART0 peripheral as the `Transport`.
fn transport_uart(uart0: peripherals::UART0) -> Transport {
    #[cfg(any(feature = "esp32", feature = "esp32s2"))]
    let clock_source = ClockSource::Apb;
    #[cfg(not(any(feature = "esp32", feature = "esp32s2")))]
    let clock_source = ClockSource::Xtal;

    let uart_config = Config::default().with_clock_source(clock_source);

    let mut serial = Uart::new(uart0, uart_config)
        .unwrap()
        .with_rx(NoPin)
        .with_tx(NoPin);
    serial.set_interrupt_handler(uart0_handler);
    serial.listen(UartInterrupt::RxFifoFull);

    static mut TRANSPORT: StaticCell<Uart<'static, Blocking>> = StaticCell::new();

    Transport::Uart(unsafe { TRANSPORT.init(serial) })
}

// Initialize the USB Serial JTAG peripheral as the `Transport`.
#[cfg(usb_device)]
fn transport_usb_serial_jtag(usb_device: peripherals::USB_DEVICE) -> Transport {
    use flasher_stub::{
        hal::usb_serial_jtag::UsbSerialJtag,
        io::usb_serial_jtag::usb_device_handler,
    };

    let mut usb_serial = UsbSerialJtag::new(usb_device);
    usb_serial.set_interrupt_handler(usb_device_handler);
    usb_serial.listen_rx_packet_recv_interrupt();

    static mut TRANSPORT: StaticCell<UsbSerialJtag<'static, Blocking>> = StaticCell::new();

    Transport::UsbSerialJtag(unsafe { TRANSPORT.init(usb_serial) })
}
