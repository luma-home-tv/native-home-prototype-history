//! Fixed, read-only capability probe from the normal app identity. No caller-selected URI,
//! application launch, subscription or setting write can be requested through this module.
#[cfg(target_arch = "arm")]
pub fn collect() -> serde_json::Value {
    use plx_platform::webos::ls2;
    use serde_json::json;
    let bus = match ls2::register() {
        Ok(bus) => bus,
        Err(error) => {
            return json!({"registered":false,"error":error.to_string(),"uid":unsafe{libc::geteuid()},"writes":false})
        }
    };
    let mut value = read(&bus, plx_base::paths::app_id(), true);
    value["registered"] = serde_json::json!(true);
    value["uid"] = serde_json::json!(unsafe { libc::geteuid() });
    value
}
#[cfg(target_arch = "arm")]
pub(crate) fn read(
    bus: &plx_platform::webos::ls2::Registration,
    own_id: &str,
    probe_audio: bool,
) -> serde_json::Value {
    use plx_platform::webos::ls2;
    use serde_json::json;
    let operations = [
        (
            "apps",
            "luna://com.webos.applicationManager/listApps",
            json!({}),
        ),
        (
            "launch_points",
            "luna://com.webos.applicationManager/listLaunchPoints",
            json!({}),
        ),
        (
            "inputs",
            "luna://com.webos.service.eim/getAllInputStatus",
            json!({}),
        ),
        (
            "picture",
            "luna://com.webos.service.settings/getSystemSettings",
            json!({"category":"picture","keys":["pictureMode","backlight","contrast","brightness"]}),
        ),
        (
            "audio",
            "luna://com.webos.service.audio/master/getVolume",
            json!({}),
        ),
    ];
    let mut replies = serde_json::Map::new();
    for (name, uri, payload) in operations {
        if name == "audio" && !probe_audio {
            replies.insert(
                name.to_owned(),
                json!({"returnValue":false,"known_unavailable":true}),
            );
            continue;
        }
        let response = if name == "audio" {
            // Attach only this process's real app identity, never impersonate another caller.
            bus.call_as_app(
                uri,
                &payload.to_string(),
                own_id,
                std::time::Duration::from_millis(1500),
            )
        } else {
            bus.call(
                uri,
                &payload.to_string(),
                std::time::Duration::from_millis(1500),
            )
        };
        let result = match response {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| json!({"parse_error":true})),
            Err(ls2::Fail::Timeout) => json!({"timeout":true}),
            Err(ls2::Fail::Setup {
                stage,
                detail,
                code,
            }) => json!({"setup_error":stage,"detail":detail,"code":code}),
        };
        replies.insert(name.to_owned(), result);
    }
    json!({"registered":true,"uid":unsafe{libc::geteuid()},"writes":false,"replies":replies})
}
#[cfg(not(target_arch = "arm"))]
pub fn collect() -> serde_json::Value {
    serde_json::json!({"registered":false,"unsupported_host":true,"writes":false})
}
