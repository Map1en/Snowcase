//! Render one thumbnail through unipeek to check the dependency works end to end.
//!
//! cargo run -p snowcase --example unipeek_thumb -- <package-or-fbx> [out.png]

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(source) = args.next() else {
        eprintln!("usage: unipeek_thumb <package-or-fbx> [out.png]");
        std::process::exit(64);
    };
    let out = args.next().unwrap_or_else(|| "unipeek_thumb.png".to_owned());

    println!("unipeek renderer version {}", unipeek::RENDERER_VERSION);
    let gpu = match unipeek::Gpu::headless() {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("no GPU: {error}");
            std::process::exit(3);
        }
    };
    let scene = match unipeek::Scene::load(
        unipeek::Input::from_location(&source),
        &unipeek::LoadOptions::default(),
    ) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("load failed: {error}");
            std::process::exit(3);
        }
    };
    let image = match unipeek::render_thumbnail(&gpu, &scene, &unipeek::ThumbOptions::default()) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("render failed: {error}");
            std::process::exit(3);
        }
    };
    if let Err(error) = image.save(&out) {
        eprintln!("write failed: {error}");
        std::process::exit(3);
    }
    let report = serde_json::to_string_pretty(&scene.report).unwrap_or_default();
    println!("{}x{} -> {out}\n{report}", image.width(), image.height());
}
