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
        let result = match bus.call(
            uri,
            &payload.to_string(),
            std::time::Duration::from_millis(1500),
        ) {
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
