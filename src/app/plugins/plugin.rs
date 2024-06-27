use std::error::Error;

use mlua::{Function, Lua};

use crate::app::{commands::command_info::CommandInfo, log::NotificationLevel, settings::register_key_settings_macro::{key_event_to_lua, mouse_event_to_lua}};

use super::{app_context::AppContext, event::{Event, Events}, exported_commands::ExportedCommands, popup_context::PopupContext, register_userdata::{register_settings, register_string, register_text, register_usize, register_vec_u8}};

#[derive(Debug)]
pub struct Plugin
{
    lua: Lua,
    commands: ExportedCommands,
}

impl Plugin {
    pub fn new_from_source(
        source: &str, 
        app_context: &mut AppContext
    ) -> Result<Self, Box<dyn Error>>
    {
        let lua = Lua::new();
        lua.load(source).exec()?;

        register_vec_u8(&lua)?;
        register_settings(&lua)?;
        register_text(&lua)?;
        register_string(&lua)?;
        register_usize(&lua)?;

        app_context.reset_exported_commands();
        if let Ok(init) = lua.globals().get::<_, Function>("init")
        {
            lua.scope(|scope|{
                let context = app_context.to_lua(&lua, scope);
                init.call(context)
            })?;
        }
        
        Ok(Plugin { lua , commands: app_context.take_exported_commands() })
    }

    pub fn new_from_file(
        path: &str, 
        app_context: &mut AppContext
    ) -> Result<Self, Box<dyn Error>>
    {
        let source = std::fs::read_to_string(path)?;
        Self::new_from_source(&source, app_context)
    }

    pub fn get_event_handlers(&self) -> Events
    {
        let mut handlers = Events::NONE;
        if self.lua.globals().get::<_, Function>("on_open").is_ok()
        {
            handlers |= Events::ON_OPEN;
        }
        if self.lua.globals().get::<_, Function>("on_edit").is_ok()
        {
            handlers |= Events::ON_EDIT;
        }
        if self.lua.globals().get::<_, Function>("on_save").is_ok()
        {
            handlers |= Events::ON_SAVE;
        }
        if self.lua.globals().get::<_, Function>("on_key").is_ok()
        {
            handlers |= Events::ON_KEY;
        }
        if self.lua.globals().get::<_, Function>("on_mouse").is_ok()
        {
            handlers |= Events::ON_MOUSE;
        }
        handlers
    }

    /// Handle an event, if an error occurs, return the error
    /// see [Plugin::handle] for a version that logs the error
    pub fn handle_with_error(&mut self, event: Event, app_context: &mut AppContext) -> mlua::Result<()>
    {
        app_context.set_exported_commands(self.commands.take());
        let ret = match event
        {
            Event::Open =>
            {
                // Call the on_open function
                let on_open = self.lua.globals().get::<_, Function>("on_open").unwrap();
                self.lua.scope(|scope| {
                    let context = app_context.to_lua(&self.lua, scope);
                    on_open.call::<_,()>(context)
                })
            },
            Event::Edit { new_bytes} =>
            {
                // Call the on_edit function
                let on_edit = self.lua.globals().get::<_, Function>("on_edit").unwrap();
                self.lua.scope(|scope| {
                    let new_bytes = scope.create_any_userdata_ref_mut(new_bytes)?;
                    let context = app_context.to_lua(&self.lua, scope);
                    on_edit.call::<_,()>((new_bytes, context))
                })
            },
            Event::Save =>
            {
                // Call the on_save function
                let on_save = self.lua.globals().get::<_, Function>("on_save").unwrap();
                self.lua.scope(|scope| {
                    let context = app_context.to_lua(&self.lua, scope);
                    on_save.call::<_,()>(context)
                })
            },
            Event::Key {
                event} =>
            {
                // Call the on_key function
                let on_key = self.lua.globals().get::<_, Function>("on_key").unwrap();
                let event = key_event_to_lua(&self.lua, &event).unwrap();
                self.lua.scope(|scope| {
                    let context = app_context.to_lua(&self.lua, scope);
                    on_key.call::<_,()>((event, context))
                })
            },
            Event::Mouse { event } =>
            {
                // Call the on_mouse function
                let on_mouse = self.lua.globals().get::<_, Function>("on_mouse").unwrap();
                let event = mouse_event_to_lua(&self.lua, &event).unwrap();
                self.lua.scope(|scope| {
                    let context = app_context.to_lua(&self.lua, scope);
                    on_mouse.call::<_,()>((event, context))
                })
            },
        };
        self.commands = app_context.take_exported_commands();
        ret
    }

    pub fn handle(
        &mut self, 
        event: Event,
        app_context: &mut AppContext)
    {
        if let Err(e) = self.handle_with_error(event, app_context)
        {
            app_context.logger.log(NotificationLevel::Error, &format!("In plugin: {}", e));
        }
    }

    pub fn run_command(
        &mut self, 
        command: &str, 
        app_context: &mut AppContext) -> mlua::Result<()>
    {
        let command_fn = self.lua.globals().get::<_, Function>(command)?;
        app_context.set_exported_commands(self.commands.take());
        let ret = self.lua.scope(|scope| {
            let context = app_context.to_lua(&self.lua, scope);
            command_fn.call::<_,()>(context)
        });
        self.commands = app_context.take_exported_commands();
        ret
    }

    pub fn get_commands(&self) -> &[CommandInfo]
    {
        self.commands.get_commands()
    }

    pub fn fill_popup(
        &self,
        callback: impl AsRef<str>,
        mut popup_context: PopupContext,
        mut app_context: AppContext) -> mlua::Result<()>
    {
        let callback = self.lua.globals().get::<_, Function>(callback.as_ref()).unwrap();
        self.lua.scope(|scope| {
            let popup_context = popup_context.to_lua(&self.lua, scope);
            let context = app_context.to_lua(&self.lua, scope);
            callback.call::<_,()>((popup_context, context))
        })
    }
}

#[cfg(test)]
mod test
{
    use crossterm::event::{KeyCode, KeyEvent};
    use object::Architecture;
    use ratatui::style::Style;

    use crate::{app::{log::NotificationLevel, settings::settings_value::SettingsValue, App}, get_app_context};

    use super::*;

    #[test]
    fn test_init_plugin()
    {
        let test_value = 42;
        let source = format!("
            test_value = 0
            function init(context)
                test_value = {test_value}
            end
        ");
        let mut app = App::mockup(vec![0;0x100]);
        let mut app_context = get_app_context!(app);
        let plugin = Plugin::new_from_source(&source, &mut app_context).unwrap();
        assert_eq!(plugin.lua.globals().get::<_, i32>("test_value").unwrap(), test_value);
    }

    #[test]
    fn test_discover_event_handlers()
    {
        let source = "
            function on_open(context) end
            function on_edit(new_bytes, context) end
            function on_save(context) end
            function on_key(key_event, context) end
            function on_mouse(mouse_event, context) end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        let mut app_context = get_app_context!(app);
        let plugin = Plugin::new_from_source(source, &mut app_context).unwrap();
        let handlers = plugin.get_event_handlers();
        assert_eq!(handlers, Events::ON_OPEN | Events::ON_EDIT | Events::ON_SAVE | Events::ON_KEY | Events::ON_MOUSE);
        let source = "
            function on_open(context) end
            function on_edit(new_bytes, context) end
            function on_save(context) end
        ";
        let plugin = Plugin::new_from_source(source, &mut app_context).unwrap();
        let handlers = plugin.get_event_handlers();
        assert_eq!(handlers, Events::ON_OPEN | Events::ON_EDIT | Events::ON_SAVE);
    }

    #[test]
    fn test_edit_open_data()
    {
        let source = "
            function on_open(context)
                context.data:set(0,42)
            end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        let mut app_context = get_app_context!(app);
        let mut plugin = Plugin::new_from_source(source, &mut app_context).unwrap();
        let event = Event::Open;
        plugin.handle_with_error(event, &mut app_context).unwrap();
        assert_eq!(app.data[0], 42);
    }

    #[test]
    fn test_init_change_settings()
    {
        let source = "
            function init(context)
                context.settings.color_address_selected = {fg=\"#ff0000\",bg=\"Black\"}
                context.settings.color_address_default = {fg=2}

                context.settings.key_up = {code=\"Down\",modifiers=0,kind=\"Press\",state=0}

                if context.settings:get_custom(\"test\") ~= \"Hello\" then
                    error(\"Custom setting not set\")
                end
                context.settings:set_custom(\"string\", \"World\")
                context.settings:set_custom(\"integer\", 42)
                context.settings:set_custom(\"float\", 3.14)
                context.settings:set_custom(\"boolean\", true)
                context.settings:set_custom(\"nil\", nil)
                context.settings:set_custom(\"style\", {fg=\"#ff0000\",bg=\"#000000\"})
                context.settings:set_custom(\"key\", {code=\"Up\"})
            end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        app.settings.custom.insert("test".to_string(), SettingsValue::from("Hello"));
        let mut app_context = get_app_context!(app);
        let _ = Plugin::new_from_source(source, &mut app_context).unwrap();
        assert_eq!(app.settings.color.address_selected.fg, Some(ratatui::style::Color::Rgb(0xff, 0, 0)));
        assert_eq!(app.settings.color.address_selected.bg, Some(ratatui::style::Color::Black));
        assert_eq!(app.settings.color.address_default.fg, Some(ratatui::style::Color::Indexed(2)));
        assert_eq!(app.settings.color.address_default.bg, None);
        assert_eq!(app.settings.key.up, KeyEvent::from(KeyCode::Down));
        assert_eq!(app.settings.custom.get("string").unwrap(), &SettingsValue::from("World"));
        assert_eq!(app.settings.custom.get("integer").unwrap(), &SettingsValue::from(42));
        assert_eq!(app.settings.custom.get("float").unwrap(), &SettingsValue::from(3.14));
        assert_eq!(app.settings.custom.get("boolean").unwrap(), &SettingsValue::from(true));
        assert!(app.settings.custom.get("nil").is_none());
        assert_eq!(app.settings.custom.get("style").unwrap(), &SettingsValue::from(
            Style::new()
                .fg(ratatui::style::Color::Rgb(0xff, 0, 0))
                .bg(ratatui::style::Color::Rgb(0, 0, 0))
        ));
        assert_eq!(app.settings.custom.get("key").unwrap(), &SettingsValue::from(KeyEvent::from(KeyCode::Up)));
    }

    #[test]
    fn test_on_key_with_init()
    {
        let source = "
            command = nil
            function init(context)
                command = context.settings.key_confirm
            end
            function on_key(key_event, context)
                if key_event.code == command.code then
                    context.data:set(context.offset, 42)
                end
            end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        let mut app_context = get_app_context!(app);
        let mut plugin = Plugin::new_from_source(source, &mut app_context).unwrap();

        let event = Event::Key { 
            event: KeyEvent::from(KeyCode::Down)
        };
        plugin.handle_with_error(event, &mut app_context).unwrap();
        assert_eq!(app_context.data[0], 0);
        let event = Event::Key { 
            event: app_context.settings.key.confirm, 
        };
        plugin.handle_with_error(event, &mut app_context).unwrap();
        assert_eq!(app.data[0], 42);
    }

    #[test]
    fn test_log_from_lua()
    {
        let source = "
            function init(context)
                context.log(1, \"Hello from init\")
            end

            function on_open(context)
                context.log(2, \"Hello from on_open\")
            end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        app.logger.clear();
        let mut app_context = get_app_context!(app);
        let mut plugin = Plugin::new_from_source(source, &mut app_context).unwrap();

        {
            let mut message_iter = app_context.logger.iter();
            let message = message_iter.next().unwrap();
            assert_eq!(message.level, NotificationLevel::Debug);
            assert_eq!(message.message, "Hello from init");
            assert_eq!(app_context.logger.get_notification_level(), NotificationLevel::Debug);
        }

        app_context.logger.clear();

        let event = Event::Open;
        plugin.handle_with_error(event, &mut app_context).unwrap();

        let mut message_iter = app_context.logger.iter();
        let message = message_iter.next().unwrap();
        assert_eq!(message.level, NotificationLevel::Info);
        assert_eq!(message.message, "Hello from on_open");
        assert_eq!(app_context.logger.get_notification_level(), NotificationLevel::Info);
        assert!(message_iter.next().is_none());
    }

    #[test]
    fn test_export_command()
    {
        let source = "
            function init(context)
                context.add_command(\"test\", \"Test command\")
            end
        ";
        let mut app = App::mockup(vec![0;0x100]);
        app.logger.clear();
        let mut app_context = get_app_context!(app);

        assert!(Plugin::new_from_source(source, &mut app_context).is_err(), 
            "Should not be able to export a command without defining it first");

        let source = "
            function init(context)
                context.add_command(\"test\", \"Test command\")
                context.add_command(\"test3\", \"Test command 3\")
            end

            -- Add and remove commands
            function test(context)
                context.add_command(\"test2\", \"Test command 2\")
                context.remove_command(\"test\")
            end

            -- Intentional error
            function test2(context)
                context.add_command(\"does_not_exist\", \"This command does not exist\")
            end

            -- No duplicate command should be added
            function test3(context)
                context.add_command(\"test\", \"Test command\")
                context.add_command(\"test\", \"Test command 1\")
            end
        ";

        let mut plugin = Plugin::new_from_source(source, &mut app_context).unwrap();

        let commands = plugin.commands.get_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command, "test");
        assert_eq!(commands[0].description, "Test command");
        assert_eq!(commands[1].command, "test3");
        assert_eq!(commands[1].description, "Test command 3");

        plugin.run_command(
            "test",
            &mut app_context).unwrap();
        
        let commands = plugin.commands.get_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command, "test3");
        assert_eq!(commands[0].description, "Test command 3");
        assert_eq!(commands[1].command, "test2");
        assert_eq!(commands[1].description, "Test command 2");

        assert!(plugin.run_command(
            "test2",
            &mut app_context).is_err(), 
            "Should not be able to add a command that is not defined");
        
        let commands = plugin.commands.get_commands();
        assert_eq!(commands.len(), 2, 
            "No commands should be lost when an error occurs");
        assert_eq!(commands[0].command, "test3");
        assert_eq!(commands[0].description, "Test command 3");
        assert_eq!(commands[1].command, "test2");
        assert_eq!(commands[1].description, "Test command 2");

        plugin.run_command(
            "test3",   
            &mut app_context).unwrap();

        let commands = plugin.commands.get_commands();
        assert_eq!(commands.len(), 3, 
            "No duplicate commands should be added");
        assert_eq!(commands[0].command, "test3");
        assert_eq!(commands[0].description, "Test command 3");
        assert_eq!(commands[1].command, "test2");
        assert_eq!(commands[1].description, "Test command 2");
        assert_eq!(commands[2].command, "test");
        assert_eq!(commands[2].description, "Test command 1", 
            "Should overwrite the description of the command");
    }

    #[test]
    fn test_header()
    {
        let source = "
            function on_open(context)
                context.log(1, context.header.bitness)
                context.log(1, context.header.architecture)
                context.log(1, context.header.entry_point)
            end
        ";
        
        let mut app = App::mockup(vec![0;0x100]);
        let mut app_context = get_app_context!(app);

        let mut plugin = Plugin::new_from_source(source, &mut app_context).unwrap();

        let event = Event::Open;
        plugin.handle_with_error(event, &mut app_context).unwrap();

        let messages = app_context.logger.iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[3].message, 64.to_string(), "Default bitness is 64");
        assert_eq!(messages[4].message, format!("{:?}",Architecture::Unknown), "Default architecture is Unknown");
        assert_eq!(messages[5].message, 0.to_string(), "Default entry point is 0");
    }
}