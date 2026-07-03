use image::{ColorType, ImageFormat};
use slint::ComponentHandle;
use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("visual smoke failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "software");
    }

    let output_dir = std::env::var_os("DAIS_DESK_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/dais-desk-screenshots")
        });
    std::fs::create_dir_all(&output_dir)?;

    let window = dais_desk::create_test_window()?;
    window
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 760.0));
    window.show()?;

    capture(&window, &output_dir, "home")?;
    window
        .window()
        .set_size(slint::LogicalSize::new(920.0, 660.0));
    capture(&window, &output_dir, "home-min-width")?;
    window
        .window()
        .set_size(slint::LogicalSize::new(1440.0, 860.0));
    capture(&window, &output_dir, "home-wide")?;
    window
        .window()
        .set_size(slint::LogicalSize::new(1180.0, 760.0));

    window.invoke_select_screen("compose".into());
    capture(&window, &output_dir, "home-compose-media")?;
    window.invoke_select_screen("inbox".into());
    window.invoke_select_row("notification:notice-reply".into());
    capture(&window, &output_dir, "home-inbox-notifications")?;
    window.invoke_row_action("timeline:ada-week-friday-space-news".into(), "Save".into());
    capture(&window, &output_dir, "workflow-save-post")?;
    window.invoke_select_screen("today".into());
    capture(&window, &output_dir, "home-today")?;
    window.invoke_select_screen("conversations".into());
    capture(&window, &output_dir, "home-conversations")?;

    window.invoke_select_screen("inbox".into());
    window.invoke_row_action("notification:notice-reply".into(), "Reply".into());
    capture(&window, &output_dir, "workflow-reply-compose")?;

    window.invoke_select_mode("people".into());
    window.invoke_select_screen("find".into());
    capture(&window, &output_dir, "people-find-search")?;

    window.invoke_select_screen("friends".into());
    capture(&window, &output_dir, "people-friends")?;
    window.invoke_select_screen("followers".into());
    capture(&window, &output_dir, "people-followers")?;
    window.invoke_select_screen("following".into());
    capture(&window, &output_dir, "people-following")?;
    window.invoke_select_mode("people".into());
    window.invoke_select_screen("followers".into());
    window.invoke_row_action(
        "follower:https://new.example/users/follower".into(),
        "Approve".into(),
    );
    capture(&window, &output_dir, "workflow-follower-approve")?;

    window.hide()?;
    Ok(())
}

fn capture(
    window: &dais_desk::MainWindow,
    output_dir: &Path,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    slint::platform::update_timers_and_animations();
    let snapshot = window.window().take_snapshot()?;
    let width = snapshot.width();
    let height = snapshot.height();
    let mut bytes = snapshot.as_bytes().to_vec();

    assert!(width >= 900, "{name} snapshot width is too small: {width}");
    assert!(
        height >= 600,
        "{name} snapshot height is too small: {height}"
    );
    assert_eq!(
        bytes.len(),
        width as usize * height as usize * 4,
        "{name} snapshot byte length does not match RGBA dimensions"
    );

    let transparent_pixels = bytes.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
    if transparent_pixels * 2 > width as usize * height as usize {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }

    let mut sampled_colors = BTreeSet::new();
    let stride = ((width as usize * height as usize) / 6000).max(1);
    for pixel in bytes.chunks_exact(4).step_by(stride) {
        sampled_colors.insert([pixel[0], pixel[1], pixel[2], pixel[3]]);
    }
    assert!(
        sampled_colors.len() > 32,
        "{name} snapshot looks visually blank: only {} sampled colors",
        sampled_colors.len()
    );
    assert!(
        bytes
            .chunks_exact(4)
            .any(|pixel| pixel[0] < 245 && pixel[1] < 245 && pixel[2] < 245),
        "{name} snapshot is missing visible foreground content"
    );

    let path = output_dir.join(format!("{name}.png"));
    image::save_buffer_with_format(
        &path,
        &bytes,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    println!("wrote {}", path.display());
    Ok(())
}
