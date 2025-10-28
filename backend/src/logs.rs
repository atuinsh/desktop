use fern::colors::{Color, ColoredLevelConfig};
use fern::Dispatch;

pub fn stderr_log_dispatcher_factory() -> Dispatch {
    Dispatch::new().format(move |out, message, record| {
        let colors_line = get_colors_line();

        out.finish(format_args!(
            "{color_reset}[{date}][{level}]{target}{color_line} {message}{color_reset}",
            color_reset = format_args!("\x1B[0m"),
            color_line = format_args!(
                "\x1B[{}m",
                colors_line.get_color(&record.level()).to_fg_str()
            ),
            date = humantime::format_rfc3339(std::time::SystemTime::now()),
            target = format_args!("\x1B[{}m[{}]", Color::White.to_fg_str(), record.target()),
            level = colors_line.color(record.level()),
            message = message
        ));
    })
}

pub fn log_dir_dispatcher_factory() -> Dispatch {
    todo!()
}

fn get_colors_line() -> ColoredLevelConfig {
    ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightWhite)
        .debug(Color::Blue)
        .trace(Color::BrightBlack)
}
