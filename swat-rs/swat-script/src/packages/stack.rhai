fn stack_frame_total(ctx) {
    ctx.stack_frame_count()
}

fn stack_top_label(ctx) {
    ctx.stack_frame_label(0)
}

fn stack_has_local(ctx, frame, name) {
    let count = ctx.stack_frame_local_count(frame);
    let index = 0;
    while index < count {
        if ctx.stack_frame_local_name(frame, index) == name {
            return true;
        }
        index += 1;
    }
    false
}

fn stack_has_register(ctx, frame, name) {
    let count = ctx.stack_frame_register_count(frame);
    let index = 0;
    while index < count {
        if ctx.stack_frame_register_name(frame, index) == name {
            return true;
        }
        index += 1;
    }
    false
}
