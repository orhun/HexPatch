function init(settings, context)
    context:add_command("p2c1", "Plugin 2 Command 1")
    context:add_command("p2c2", "Plugin 2 Command 2")
end

function p2c1(data, offset, current_instruction, context, header)
    context:log(1, "Plugin 2 Command 1 called")
end

function p2c2(data, offset, current_instruction, context, header)
    context:log(1, "Plugin 2 Command 2 called")
end