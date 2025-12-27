use cardputer::{
    hotkeys,
    os::chainload,
    runtime,
    terminal::OwnedTerminal,
    typing::{KeyboardEvent, Typing},
    SCREEN_HEIGHT, SCREEN_WIDTH,
};

#[allow(clippy::approx_constant)]
fn main() {
    runtime::init();

    let (mut p, _modem) = runtime::take_cardputer();

    let mut terminal = OwnedTerminal::<SCREEN_WIDTH, SCREEN_HEIGHT>::new(&mut p.display);

    let mut typing = Typing::new();

    let mut ctx = simple_context_().unwrap();

    loop {
        if let Some(hotkeys::SystemAction::ReturnToOs) = hotkeys::poll_action(&mut p.keyboard) {
            chainload::reboot_to_factory();
        }

        let evt = p.keyboard.read_events();
        if let Some(evt) = evt {
            if let Some(evts) = typing.eat_keyboard_events(evt) {
                match evts {
                    KeyboardEvent::Ascii(c) => {
                        terminal.command_line_mut().push(c);
                    }
                    KeyboardEvent::Backspace => {
                        terminal.command_line_mut().pop();
                    }
                    KeyboardEvent::Enter => {
                        let line = terminal.command_line_mut().get().to_string();
                        let res = execute_command(&line, &mut ctx);
                        terminal.enter();
                        terminal.println(&res);
                    }
                    KeyboardEvent::ArrowUp => {
                        terminal.command_line_mut().arrow_up();
                    }
                    _ => {}
                }
            }
        }

        terminal.draw();
    }
}

fn execute_command(command: &str, ctx: &mut rink_core::Context) -> String {
    use rink_core::*;

    //let mut ctx = Context::new();

    let result = one_line(ctx, command);

    match result {
        Ok(r) => r,
        Err(r) => r,
    }
}

pub fn simple_context_() -> Result<rink_core::Context, String> {
    use rink_core::*;

    use rink_core::loader::gnu_units;

    let units = include_str!("definitions.units");

    let mut iter = gnu_units::TokenIterator::new(units).peekable();
    let units = gnu_units::parse(&mut iter);

    //let dates = parsing::datetime::parse_datefile(DATES_FILE);

    let mut ctx = Context::new();
    ctx.load(units)?;
    //ctx.load_dates(dates);

    Ok(ctx)
}
