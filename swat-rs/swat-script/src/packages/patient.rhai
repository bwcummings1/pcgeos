fn patient_total(ctx) {
    ctx.patient_count()
}

fn patient_has(ctx, name) {
    let count = ctx.patient_count();
    let index = 0;
    while index < count {
        if ctx.patient_name(index) == name {
            return true;
        }
        index += 1;
    }
    false
}

fn patient_handle_total(ctx, name) {
    ctx.patient_handle_count(name)
}
