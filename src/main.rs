use async_executor::Executor;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::sys;

use esp_idf_svc::hal::uart::config::Config;
use esp_idf_svc::hal::uart::AsyncUartDriver;
use esp_idf_svc::hal::uart::UartDriver;
use esp_idf_svc::io::asynch::Write;
use esp_idf_svc::log::EspLogger;
use futures_lite::future;
use gimbal_motion::timing::sleep_ms;
use gimbal_motion::unsafe_send::UnsafeSendFut;

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");

const DRIVE_TEETH: u16 = 16;
const TILT_TEETH: u16 = 160;
const PAN_TEETH: u16 = 128;

pub struct TmcRegisters {
    // gconf: tmc2209::reg::GCONF,
    // vactual: tmc2209::reg::VACTUAL,
}

type StaticDriver = AsyncUartDriver<'static, UartDriver<'static>>;

fn main() -> anyhow::Result<()> {
    // @warn. See https://github.com/esp-rs/esp-idf-template/issues/71
    sys::link_patches();
    EspLogger::initialize_default();

    let executor = Executor::new();

    // setup peripherals
    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // let _motor_conf_coolconf = tmc2209::reg::COOLCONF::default();
    // let mut motor_conf_gconf = tmc2209::reg::GCONF::default();
    // let _interface_access_counter = tmc2209::reg::IFCNT::default();
    // motor_conf_gconf.set_shaft(true); // spin motor.
    // motor_conf_gconf.set_pdn_disable(true);
    // let _vactual = tmc2209::reg::VACTUAL::default();

    let motor_driver: &'static mut StaticDriver = Box::leak(Box::new(
        AsyncUartDriver::new(
            peripherals.uart1,
            pins.gpio17,
            pins.gpio18,
            AnyIOPin::none(),
            AnyIOPin::none(),
            &Config::new().baudrate(115200.into()),
        )
        .unwrap(),
    ));

    let (mut mtx, mrx) = motor_driver.split();

    future::block_on(executor.run({
        let writer_task = async move {
            log::info!("Starting motor writer thread");
            loop {
                let msg = "hello\0".as_bytes();
                mtx.write(msg).await.unwrap();
                mtx.flush().await.unwrap();
                log::info!("wrote hello");
                sleep_ms(5000).await;
            }
        };

        let reader_task = async move {
            log::info!("Starting motor reader thread");
            loop {
                let mut buf = [0u8; 16];
                match mrx.read(&mut buf).await {
                    Ok(num_bytes) => match std::str::from_utf8(&buf[0..num_bytes]) {
                        Ok(s) => log::info!("read buf: {}", s),
                        Err(e) => {
                            log::info!("err: {:?} {:?}", e, buf);
                            panic!("kaboom");
                        }
                    },
                    Err(e) => log::error!("failed to read: {:?}", e),
                }
                sleep_ms(100).await;
            }
        };

        future::or(
            executor.spawn(UnsafeSendFut::new(writer_task)),
            executor.spawn(UnsafeSendFut::new(reader_task)),
        )
    }));

    // let gimbal_pins = GimbalBuilder::pan_dir(pins.gpio14.downgrade_output().into())
    //     .pan_step(pins.gpio15.downgrade_output().into())
    //     .tilt_dir(pins.gpio21.downgrade_output().into())
    //     .tilt_step(pins.gpio26.downgrade_output().into())
    //     .pan_endstop(pins.gpio30.downgrade().into())
    //     .tilt_endstop(pins.gpio31.downgrade().into());

    // let cmds_arc: Arc<Mutex<VecDeque<Cmd>>> = Arc::new(Mutex::new(VecDeque::new()));
    // let cmds_reader = cmds_arc.clone();

    // let gimbal_arc: Arc<Mutex<Gimbal>> = Arc::new(Mutex::new(Gimbal::new(
    //     gimbal_pins,
    //     PAN_TEETH,
    //     DRIVE_TEETH,
    //     TILT_TEETH,
    //     DRIVE_TEETH,
    //     30.,
    //     30.,
    // )));

    // let mut wifi = create_wifi(peripherals.modem)?;
    // let ip_info = block_on(connect_wifi(&mut wifi, SSID, PASSWORD))?;
    // let _server = server::start(ip_info, cmds_arc.clone(), gimbal_arc.clone())?;

    // loop {
    //     let cmd_opt = { cmds_reader.lock().unwrap().borrow_mut().pop_front() };

    //     if let Some(cmd) = cmd_opt {
    //         match cmd {
    //             Cmd::ClearCmdQueue => {
    //                 let mut cmds = cmds_reader.lock().unwrap();
    //                 cmds.clear();
    //             }
    //             Cmd::ProcessGcode(mv) => {
    //                 let mut gimbal = gimbal_arc.lock().unwrap();
    //                 if gimbal.last_error_message.is_none() {
    //                     match gimbal.process_gcode(mv) {
    //                         Ok(_) => {}
    //                         Err(e) => {
    //                             gimbal.last_error_message = Some(e.to_string());
    //                             log::error!("failed to process gcode: {e}. restart required");
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }

    //     FreeRtos::delay_ms(100);
    Ok(())
}
