use {
    crate::{
        cmd::Cmd, error::ServerError, gcode::GcodeParser, gimbal::Gimbal, server_response::Response,
    },
    embedded_svc::{http::Headers, io::Write, ipv4::IpInfo},
    esp_idf_svc::{
        http::{server::EspHttpServer, Method},
        io::EspIOError,
    },
    log::info,
    serde_json,
    std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    },
};

#[derive(serde::Deserialize, serde::Serialize)]
struct PostGcode {
    pub gcode: String,
}

pub fn start(
    ip_info: IpInfo,
    state: Arc<Mutex<VecDeque<Cmd>>>,
    gimbal_arc: Arc<Mutex<Gimbal>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let ip = ip_info.ip;
    info!("starting server at {ip}");
    let location = std::format!("https://cdaringe.github.io/gimbal-gui?gimbal_url={ip}");

    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: 10240,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_configuration)?;

    server.fn_handler::<ServerError, _>("/", Method::Get, move |req| {
        let conn = req.connection().unwrap_or("unknown");
        info!("handling req from connection: {conn}");
        let mut response = req.into_response(301, None, &[("Location", &location)])?;
        response.flush()?;
        Ok(())
    })?;

    server.fn_handler::<ServerError, _>("/api/state", Method::Get, move |req| {
        let mut response = req.into_response(
            200,
            Some("Ok"),
            &[
                ("Access-Control-Allow-Origin", "*"),
                ("content-type", "application/json"),
            ],
        )?;
        write!(
            response,
            "{}",
            &Response::ok(&*gimbal_arc.lock().unwrap()).json()?
        )
        .map_err(|e| ServerError::Unknown(e.to_string()))?;
        response.flush()?;
        Ok(())
    })?;

    server.fn_handler::<ServerError, _>("/api/restart", Method::Get, move |_req| {
        esp_idf_svc::hal::reset::restart();
        Ok(())
    })?;

    server.fn_handler::<ServerError, _>("/api/gcode", Method::Post, move |mut req| {
        let json_str = {
            let mut buf = [0; 256];
            req.read(&mut buf)?;
            String::from_utf8(buf.into())
                .map_err(|e| ServerError::BadInput(e.to_string()))?
                .trim_end_matches('\0')
                .to_string()
        };

        let body: PostGcode = serde_json::from_str(&json_str)?;

        let (code, message, payload) = match GcodeParser::of_str(&body.gcode) {
            Ok(gcode) => {
                state.lock().unwrap().push_back(Cmd::ProcessGcode(gcode));
                (200, "ok", Response::ok(true).json()?)
            }
            Err(err) => (400, "bad input", Response::error(err.to_string()).json()?),
        };
        let mut response = req.into_response(
            code,
            Some(message),
            &[
                ("Access-Control-Allow-Origin", "*"),
                ("content-type", "application/json"),
            ],
        )?;
        response.write(payload.as_bytes())?;
        response.flush()?;
        Ok(())
    })?;

    // server.ws_handler("/ws", move |ws| {
    //     let conn = ws.connection().unwrap_or("unknown");
    //     info!("handling ws req from connection: {conn}");
    //     let gimbal_arc = gimbal_arc.clone();
    //     ws.on_message(move |msg| {
    //         let msg = msg?;
    //         let cmd: Cmd = serde_json::from_str(&msg)?;
    //         let mut gimbal = gimbal_arc.lock()?;
    //         gimbal.queue.push_back(cmd);
    //         Ok(())
    //     })?;
    // })?;

    Ok(server)
}
