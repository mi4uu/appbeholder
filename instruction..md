# app behlder
observability app

## stack

- axum
- htmx
- tokio-postgres
- sse
- rust

## descritions

pbservability web app acting as server suporting any app that can send logs/traces to otel .
should be able to handle backend and frontend logs and errors, traces etc.
it should help to trace errors logs with traces, be able to r=trace context and stack and allow to easly trace step by step what lead to problem.
it should some endpoint to allow to send server utilization like cpu/cpus/cores, memory, processes utylization etc. to track server utilization over time, and even corelate spikes with what app was doing and what might cause spike.
it should allow to tradk multiple projects, and hosts.
products that reaize similar goals pretty good are datadog and sentry.

it should be easy to use, fast.
