use {
    esp_idf_svc::{
        hal::{
            delay::FreeRtos,
            gpio::{AnyIOPin, IOPin, OutputPin},
            peripherals::Peripherals,
            sys, uart,
        },
        log::EspLogger,
    },
    futures::executor::block_on,
    std::{
        borrow::BorrowMut,
        collections::VecDeque,
        sync::{Arc, Mutex},
    },
};

use {
    esp_idf_svc::hal::delay::TickType,
    gimbal_motion::{cmd::Cmd, gimbal_pins::GimbalBuilder},
};

use gimbal_motion::{
    gimbal::Gimbal,
    server,
    wifi::{connect_wifi, create_wifi},
};

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");

const DRIVE_TEETH: u16 = 16;
const TILT_TEETH: u16 = 160;
const PAN_TEETH: u16 = 128;

pub struct TmcRegisters {
    gconf: tmc2209::reg::GCONF,
    vactual: tmc2209::reg::VACTUAL,
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    sys::link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // let motor_conf_coolconf = tmc2209::reg::COOLCONF::default();
    let mut motor_conf_gconf = tmc2209::reg::GCONF::default();
    motor_conf_gconf.set_shaft(true); // spin motor.
    motor_conf_gconf.set_pdn_disable(true);
    let vactual = tmc2209::reg::VACTUAL::default();

    let mut motor_driver = uart::UartDriver::new(
        peripherals.uart1,
        pins.gpio17,
        pins.gpio18,
        AnyIOPin::none(),
        AnyIOPin::none(),
        &uart::config::Config::new().baudrate(115200.into()),
    )
    .unwrap();

    let (mut mtx, mrx) = motor_driver.split();

    let writer_task = async {
        log::info!("Starting motor writer thread");

        loop {
            tmc2209::send_write_request(0, motor_conf_gconf, &mut mtx).unwrap();
            tmc2209::send_write_request(0, vactual, &mut mtx).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
            log::info!("[writer] sleeping, man");
        }
    };

    let reader_task = async {
        log::info!("Starting motor reader thread");
        let mut tmc_reader = tmc2209::Reader::default();
        let mut buf = [0u8; 256];

        loop {
            match mrx.read(&mut buf, TickType::new_millis(5).into()) {
                Ok(b) => {
                    if let (_, Some(response)) = tmc_reader.read_response(&[b.try_into().unwrap()])
                    {
                        match response.crc_is_valid() {
                            true => log::info!("Received valid response!"),
                            false => {
                                log::error!("Received invalid response!");
                                continue;
                            }
                        }
                        log::debug!("{:?}", response.reg_state());

                        match response.reg_addr() {
                            Ok(tmc2209::reg::Address::IOIN) => {
                                let reg = response.register::<tmc2209::reg::IOIN>().unwrap();
                                log::info!("{:?}", reg);
                            }
                            Ok(tmc2209::reg::Address::IFCNT) => {
                                let reg = response.register::<tmc2209::reg::IFCNT>().unwrap();
                                log::info!("{:?}", reg);
                            }
                            addr => log::warn!("Unexpected register address: {:?}", addr),
                        }
                    }
                }
                Err(e) => log::error!("Error reading from motor: {:?}", e),
            }
        }
    };

    let sel = embassy_futures::join::join(reader_task, writer_task);
    embassy_futures::block_on(sel);
    log::info!("i guess the motors are done talking forever.");

    let gimbal_pins = GimbalBuilder::pan_dir(pins.gpio14.downgrade_output().into())
        .pan_step(pins.gpio15.downgrade_output().into())
        .tilt_dir(pins.gpio21.downgrade_output().into())
        .tilt_step(pins.gpio26.downgrade_output().into())
        .pan_endstop(pins.gpio30.downgrade().into())
        .tilt_endstop(pins.gpio31.downgrade().into());

    let cmds_arc: Arc<Mutex<VecDeque<Cmd>>> = Arc::new(Mutex::new(VecDeque::new()));
    let cmds_reader = cmds_arc.clone();

    let gimbal_arc: Arc<Mutex<Gimbal>> = Arc::new(Mutex::new(Gimbal::new(
        gimbal_pins,
        PAN_TEETH,
        DRIVE_TEETH,
        TILT_TEETH,
        DRIVE_TEETH,
        30.,
        30.,
    )));

    let mut wifi = create_wifi(peripherals.modem)?;
    let ip_info = block_on(connect_wifi(&mut wifi, SSID, PASSWORD))?;
    let _server = server::start(ip_info, cmds_arc.clone(), gimbal_arc.clone())?;

    loop {
        let cmd_opt = { cmds_reader.lock().unwrap().borrow_mut().pop_front() };

        if let Some(cmd) = cmd_opt {
            match cmd {
                Cmd::ClearCmdQueue => {
                    let mut cmds = cmds_reader.lock().unwrap();
                    cmds.clear();
                }
                Cmd::ProcessGcode(mv) => {
                    let mut gimbal = gimbal_arc.lock().unwrap();
                    if gimbal.last_error_message.is_none() {
                        match gimbal.process_gcode(mv) {
                            Ok(_) => {}
                            Err(e) => {
                                gimbal.last_error_message = Some(e.to_string());
                                log::error!("failed to process gcode: {e}. restart required");
                            }
                        }
                    }
                }
            }
        }

        FreeRtos::delay_ms(100);
    }
}
