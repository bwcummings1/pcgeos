fn process_event_total(ctx) {
    ctx.event_count()
}

fn process_has_summary(ctx, needle) {
    ctx.summary_search_count(needle) > 0
}

fn process_query_total(ctx, expr) {
    ctx.query_count(expr)
}
