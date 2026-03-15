fn handle_total(ctx) {
    ctx.handle_count()
}

fn handle_has(ctx, handle) {
    let count = ctx.handle_count();
    let index = 0;
    while index < count {
        if ctx.handle_name(index) == handle {
            return true;
        }
        index += 1;
    }
    false
}

fn handle_object_total(ctx, handle) {
    ctx.handle_object_count(handle)
}

fn resource_total(ctx) {
    ctx.resource_count()
}

fn resource_has(ctx, resource) {
    let count = ctx.resource_count();
    let index = 0;
    while index < count {
        if ctx.resource_name(index) == resource {
            return true;
        }
        index += 1;
    }
    false
}

fn resource_object_total(ctx, resource) {
    ctx.resource_object_count(resource)
}

fn object_total(ctx) {
    ctx.object_count()
}

fn object_has(ctx, object) {
    let count = ctx.object_count();
    let index = 0;
    while index < count {
        if ctx.object_identity(index) == object {
            return true;
        }
        index += 1;
    }
    false
}

fn object_class_is(ctx, object, class_name) {
    ctx.object_class(object) == class_name
}
