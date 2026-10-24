//! Spanish for what the backend shows: error messages, the tray menu, window titles and format names.
//! English text is the key; `{}` placeholders are filled in order by `tr!`.
use std::sync::atomic::{AtomicBool, Ordering};

static SPANISH: AtomicBool = AtomicBool::new(false);

/// `pref` is the Settings value: "auto" (system language), "en" or "es".
pub fn set_language(pref: &str) {
    let lang = if pref == "es" || pref == "en" { pref.to_string() } else { sys_locale::get_locale().unwrap_or_default() };
    SPANISH.store(lang.to_ascii_lowercase().starts_with("es"), Ordering::Relaxed);
}

pub fn t(en: &'static str) -> &'static str {
    if !SPANISH.load(Ordering::Relaxed) {
        return en;
    }
    ES.iter().find(|(k, _)| *k == en).map(|(_, v)| *v).unwrap_or(en)
}

/// Replace each `{}` in `template` with the next argument.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    let mut args = args.iter();
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        out.push_str(args.next().map(String::as_str).unwrap_or(""));
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// Translated message as a `String`, from a literal and optional `{}` arguments.
#[macro_export]
macro_rules! tr {
    ($s:literal) => {
        $crate::i18n::t($s).to_string()
    };
    ($s:literal, $($a:expr),+ $(,)?) => {
        {
            let args: Vec<String> = vec![$(($a).to_string()),+];
            $crate::i18n::fill($crate::i18n::t($s), &args)
        }
    };
}

const ES: &[(&str, &str)] = &[
    // tray and windows
    ("Capture…", "Capturar…"),
    ("Toggle camera", "Mostrar u ocultar cámara"),
    ("History", "Historial"),
    ("Settings", "Configuración"),
    ("Show / hide the moon", "Mostrar u ocultar la luna"),
    ("Quit myCapture", "Salir de myCapture"),
    ("myCapture — click to capture", "myCapture: haz clic para capturar"),
    ("myCapture — Preview", "myCapture: Vista previa"),
    ("myCapture — History", "myCapture: Historial"),
    ("myCapture — Settings", "myCapture: Configuración"),
    ("PNG image", "Imagen PNG"),
    ("Animated GIF", "GIF animado"),
    ("WebM video", "Video WebM"),
    ("MP4 video", "Video MP4"),
    // capture and recording
    ("Could not capture the screen.", "No se pudo capturar la pantalla."),
    ("A recording is in progress.", "Hay una grabación en curso."),
    ("No capture in progress", "No hay ninguna captura en curso"),
    ("Selection is too small", "La selección es demasiado pequeña"),
    ("Capture not found", "No se encontró la captura"),
    ("Could not use the folder {}", "No se pudo usar la carpeta {}"),
    ("Not recording", "No se está grabando"),
    ("Recording could not start.\n\n{}", "No se pudo iniciar la grabación.\n\n{}"),
    ("Could not register hotkey {}: {}", "No se pudo registrar el atajo {}: {}"),
    ("Desktop sound: {}", "Sonido del escritorio: {}"),
    ("Desktop sound: the audio device did not respond.", "Sonido del escritorio: el dispositivo de audio no respondió."),
    ("ffmpeg was not found ({}). Install it or set its path in Settings.", "No se encontró ffmpeg ({}). Instálalo o indica su ruta en Configuración."),
    ("Could not start ffmpeg ({}). Install it or set its path in Settings.", "No se pudo iniciar ffmpeg ({}). Instálalo o indica su ruta en Configuración."),
    ("ffmpeg stopped unexpectedly.", "ffmpeg se detuvo inesperadamente."),
    ("Nothing was recorded.", "No se grabó nada."),
    ("Nothing was captured.", "No se capturó nada."),
    ("A scrolling capture is in progress.", "Hay una captura con desplazamiento en curso."),
    ("Could not finish the video. The raw recording is kept in {}", "No se pudo terminar el video. La grabación sin procesar se guardó en {}"),
    ("{} (included with myCapture)", "{} (incluido con myCapture)"),
    ("Unknown service", "Servicio desconocido"),
    ("Could not check for updates: {}", "No se pudo buscar actualizaciones: {}"),
    ("Check for updates first.", "Primero busca actualizaciones."),
    ("The update could not be installed: {}", "No se pudo instalar la actualización: {}"),
    // sharing
    ("Upload failed: {}", "No se pudo subir: {}"),
    ("Server said {}: {}", "El servidor respondió {}: {}"),
    ("Server response had no url", "La respuesta del servidor no incluía la URL"),
    ("No `{}` in response: {}", "No hay `{}` en la respuesta: {}"),
    ("No upload destination is configured yet. Open Settings → Sharing to pick one.", "Todavía no hay un destino para subir. Abre Configuración → Compartir para elegir uno."),
    ("Set your myCapture server URL in Settings → Sharing.", "Indica la URL de tu servidor myCapture en Configuración → Compartir."),
    ("Set the upload URL in Settings → Sharing.", "Indica la URL de subida en Configuración → Compartir."),
    ("S3 storage", "Almacenamiento S3"),
    ("Custom upload", "Subida personalizada"),
    ("myCapture server", "Servidor myCapture"),
    // cloud drives
    ("{} needs an app key first. In Settings → Sharing → {}, open “Use your own {} app” and follow the steps.", "{} necesita primero una clave de aplicación. En Configuración → Compartir → {}, abre “Usar tu propia aplicación de {}” y sigue los pasos."),
    ("Port {} is busy, so the sign-in can't come back to myCapture. Close the other sign-in and try again.", "El puerto {} está ocupado, así que el inicio de sesión no puede volver a myCapture. Cierra el otro inicio de sesión e inténtalo de nuevo."),
    ("Sign-in timed out. Click Connect to try again.", "Se agotó el tiempo para iniciar sesión. Haz clic en Conectar para volver a intentarlo."),
    ("The sign-in returned no access token.", "El inicio de sesión no devolvió un token de acceso."),
    ("The sign-in returned no refresh token.", "El inicio de sesión no devolvió un token de renovación."),
    ("Sign-in was cancelled.", "Se canceló el inicio de sesión."),
    ("{} sign-in failed: {}", "No se pudo iniciar sesión en {}: {}"),
    ("{} is connected", "{} está conectado"),
    ("You can close this tab and go back to myCapture.", "Puedes cerrar esta pestaña y volver a myCapture."),
    ("Sign-in didn't finish", "El inicio de sesión no terminó"),
    ("{} sign-in", "Inicio de sesión en {}"),
    ("{} failed ({}): {}", "{} falló ({}): {}"),
    ("{}: unexpected reply ({})", "{}: respuesta inesperada ({})"),
    ("{} isn't connected. Open Settings → Sharing → {} and click Connect.", "{} no está conectado. Abre Configuración → Compartir → {} y haz clic en Conectar."),
    ("{}\n\nIf you removed myCapture's access, connect {} again in Settings.", "{}\n\nSi quitaste el acceso de myCapture, vuelve a conectar {} en Configuración."),
    ("No access token in the reply.", "La respuesta no incluía un token de acceso."),
    ("Account lookup", "Consulta de la cuenta"),
    ("Dropbox upload", "Subida a Dropbox"),
    ("Dropbox gave no upload session.", "Dropbox no abrió una sesión de subida."),
    ("Dropbox upload: empty file", "Subida a Dropbox: archivo vacío"),
    ("Dropbox did not say where it saved the file.", "Dropbox no indicó dónde guardó el archivo."),
    ("Dropbox could not make a share link: {}", "Dropbox no pudo crear un enlace para compartir: {}"),
    ("Dropbox share link", "Enlace de Dropbox"),
    ("Dropbox returned no link.", "Dropbox no devolvió un enlace."),
    ("OneDrive upload", "Subida a OneDrive"),
    ("OneDrive gave no upload address.", "OneDrive no dio una dirección de subida."),
    ("OneDrive did not confirm the upload.", "OneDrive no confirmó la subida."),
    ("OneDrive share link", "Enlace de OneDrive"),
    ("OneDrive returned no link.", "OneDrive no devolvió un enlace."),
    ("Google Drive folder", "Carpeta de Google Drive"),
    ("Google Drive did not create the folder.", "Google Drive no creó la carpeta."),
    ("Google Drive upload", "Subida a Google Drive"),
    ("Google Drive gave no upload address.", "Google Drive no dio una dirección de subida."),
    ("Google Drive did not confirm the upload.", "Google Drive no confirmó la subida."),
    ("Google Drive share link", "Enlace de Google Drive"),
    // video formats (Settings → Recording)
    ("Software", "Software"),
    ("Other", "Otros"),
    ("Hardware (GPU)", "Hardware (GPU)"),
    ("H.264 MP4", "H.264 MP4"),
    ("Plays everywhere: WhatsApp, Teams, browsers, phones.", "Se reproduce en todas partes: WhatsApp, Teams, navegadores y teléfonos."),
    ("About half the size of H.264. Plays on recent Windows, macOS, iPhone and Android; some web apps can't preview it.", "Ocupa cerca de la mitad que H.264. Se reproduce en Windows, macOS, iPhone y Android recientes; algunas aplicaciones web no pueden mostrarlo."),
    ("Smallest files. Needs recent browsers and apps; converting takes longer when you finish.", "Los archivos más pequeños. Requiere navegadores y aplicaciones recientes; la conversión al terminar tarda más."),
    ("Open web format with Opus audio. Good for websites; not accepted by WhatsApp.", "Formato web abierto con audio Opus. Bueno para sitios web; WhatsApp no lo acepta."),
    ("Animated GIF", "GIF animado"),
    ("Loops inline in any chat or email. No sound, and best kept short.", "Se repite dentro de cualquier chat o correo. Sin sonido; mejor si es corto."),
    ("H.264 encoded on an NVIDIA graphics card: very light on the CPU.", "H.264 codificado en una tarjeta gráfica NVIDIA: casi no usa el procesador."),
    ("H.264 encoded on Intel graphics: very light on the CPU.", "H.264 codificado en gráficos Intel: casi no usa el procesador."),
    ("H.264 encoded on an AMD graphics card: very light on the CPU.", "H.264 codificado en una tarjeta gráfica AMD: casi no usa el procesador."),
    ("H.264 encoded by the Mac's media engine.", "H.264 codificado por el motor multimedia del Mac."),
    ("Small HEVC files encoded on an NVIDIA graphics card.", "Archivos HEVC pequeños codificados en una tarjeta gráfica NVIDIA."),
    ("Small HEVC files encoded on Intel graphics.", "Archivos HEVC pequeños codificados en gráficos Intel."),
    ("Small HEVC files encoded on an AMD graphics card.", "Archivos HEVC pequeños codificados en una tarjeta gráfica AMD."),
    ("Small HEVC files encoded by the Mac's media engine.", "Archivos HEVC pequeños codificados por el motor multimedia del Mac."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_in_order() {
        assert_eq!(fill("{} of {}", &["1".into(), "3".into()]), "1 of 3");
        assert_eq!(fill("no args", &[]), "no args");
    }

    #[test]
    fn placeholders_match() {
        for (en, es) in ES {
            assert_eq!(en.matches("{}").count(), es.matches("{}").count(), "{en}");
        }
    }

    /// Every `tr!` literal in the sources has a Spanish entry.
    #[test]
    fn every_message_is_translated() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = vec![];
        for entry in std::fs::read_dir(dir).unwrap() {
            let src = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for chunk in src.split("tr!(\"").skip(1) {
                let mut key = String::new();
                let mut chars = chunk.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => key.push('\n'),
                            Some(o) => key.push(o),
                            None => {}
                        },
                        _ => key.push(c),
                    }
                }
                if !key.is_empty() && !ES.iter().any(|(k, _)| *k == key) {
                    missing.push(key);
                }
            }
        }
        assert!(missing.is_empty(), "missing Spanish for: {missing:#?}");
    }
}
