fn source_function_total(ctx) {
    ctx.source_function_count()
}

fn source_has_function(ctx, function) {
    let count = ctx.source_function_count();
    let index = 0;
    while index < count {
        if ctx.source_function_name(index) == function {
            return true;
        }
        index += 1;
    }
    false
}

fn source_function_event_total(ctx, function) {
    ctx.source_function_event_count(function)
}

fn source_file_total(ctx) {
    ctx.source_file_count()
}

fn source_has_file(ctx, file) {
    ctx.source_file_event_count(file) > 0
}

fn source_file_event_total(ctx, file) {
    ctx.source_file_event_count(file)
}

fn source_view_has(ctx, file, line, before, after, needle) {
    ctx.source_view_contains(file, line, before, after, needle)
}
