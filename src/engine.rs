use rquickjs::context::EvalOptions;
use rquickjs::{CatchResultExt, Context, Ctx, FromJs, Function, Object, Runtime};
use serde_json::Value;

const HOST: &str = include_str!("../host/host.js");

pub struct Summary {
    pub count: usize,
    pub start: i64,
    pub end: i64,
    pub mythic: i64,
}

#[derive(Clone, Copy, Default)]
pub struct Scan<'a> {
    pub raids: &'a [(i64, i64)],
    pub scanning: bool,
}

pub struct Engine {
    rt: Runtime,
    ctx: Context,
    pub version: i64,
}

fn sloppy<'js, T: FromJs<'js>>(ctx: &Ctx<'js>, code: &str) -> rquickjs::Result<T> {
    let mut options = EvalOptions::default();
    options.strict = false;
    ctx.eval_with_options(code, options)
}

fn run(ctx: &Ctx, code: &str) -> Result<(), String> {
    sloppy::<()>(ctx, code).catch(ctx).map_err(|e| e.to_string())
}

impl Engine {
    pub fn load(scripts: Vec<String>) -> Result<Self, String> {
        let rt = Runtime::new().map_err(|e| e.to_string())?;
        rt.set_max_stack_size(8 << 20);
        let ctx = Context::full(&rt).map_err(|e| e.to_string())?;

        ctx.with(|ctx| {
            run(&ctx, HOST)?;
            for script in scripts {
                if let Err(e) = run(&ctx, &script) {
                    crate::log!("parser page script: {}", e.lines().next().unwrap_or_default());
                }
            }
            Ok::<_, String>(())
        })?;

        let mut engine = Self { rt, ctx, version: 0 };
        engine.pump();

        if !engine.eval::<bool>("__listeners.length > 0")? {
            return Err("parser page did not register a message handler".into());
        }

        engine.ask("get-parser-version", |_| Ok(()))?;
        engine.version = engine.eval("__version()")?;
        Ok(engine)
    }

    fn eval<T: for<'js> FromJs<'js>>(&self, code: &str) -> Result<T, String> {
        self.ctx
            .with(|ctx| sloppy::<T>(&ctx, code).catch(&ctx).map_err(|e| e.to_string()))
    }

    fn pump(&self) {
        self.ctx.with(|ctx| {
            loop {
                while ctx.execute_pending_job() {}
                if !sloppy::<bool>(&ctx, "__timers.length > 0").unwrap_or(false) {
                    break;
                }
                let _ = sloppy::<()>(&ctx, "__drain()");
            }
        });
    }

    fn ask(&self, name: &str, fill: impl for<'js> FnOnce(&Object<'js>) -> rquickjs::Result<()>) -> Result<(), String> {
        self.ctx.with(|ctx| {
            let send = || {
                let message = Object::new(ctx.clone())?;
                message.set("message", name)?;
                message.set("id", 1)?;
                fill(&message)?;

                let send: Function = ctx.globals().get("__send")?;
                send.call::<_, ()>((message,))
            };
            send().catch(&ctx).map_err(|e| format!("{name}: {e}"))
        })?;

        self.pump();
        match self.eval::<bool>(&format!("__take('{name}-completed')"))? {
            true => Ok(()),
            false => Err(format!("parser did not answer {name}")),
        }
    }

    pub fn reset(&self) -> Result<(), String> {
        self.ask("clear-fights", |_| Ok(()))?;
        self.ask("clear-state", |_| Ok(()))
    }

    pub fn start_date(&self, date: &str) -> Result<(), String> {
        self.ask("set-start-date", |m| m.set("startDate", date))
    }

    pub fn report_code(&self, code: &str) -> Result<(), String> {
        self.ask("set-report-code", |m| m.set("reportCode", code))
    }

    pub fn live_start(&self, ms: i64) -> Result<(), String> {
        self.ask("set-live-logging-start-time", |m| m.set("startTime", ms as f64))
    }

    pub fn parse(&self, text: String, region: &Value, file: &str, span: (u64, u64), scan: Scan) -> Result<(), String> {
        let region = region.as_i64().unwrap_or(0) as f64;
        self.ask("parse-lines", |m| {
            let raids = rquickjs::Array::new(m.ctx().clone())?;
            for (i, &(start, end)) in scan.raids.iter().enumerate() {
                let raid = Object::new(m.ctx().clone())?;
                raid.set("start", start as f64)?;
                raid.set("end", end as f64)?;
                raids.set(i, raid)?;
            }

            let position = Object::new(m.ctx().clone())?;
            position.set("filePath", file)?;
            position.set("startingPosition", span.0 as f64)?;
            position.set("currentPosition", span.1 as f64)?;

            m.set("text", text)?;
            m.set("selectedRegion", region)?;
            m.set("raidsToUpload", raids)?;
            m.set("scanning", scan.scanning)?;
            m.set("logFilePosition", position)
        })?;

        let error: String = self.eval("__parseError()")?;
        match error.is_empty() {
            true => Ok(()),
            false => Err(error),
        }
    }

    pub fn fights(&self, push: bool) -> Result<Summary, String> {
        self.ask("collect-fights", |m| {
            m.set("pushFightIfNeeded", push)?;
            m.set("scanningOnly", false)
        })?;

        let summary: Value = serde_json::from_str(&self.eval::<String>("__summary()")?).map_err(|e| e.to_string())?;
        let num = |k: &str| summary[k].as_f64().unwrap_or(0.0) as i64;
        Ok(Summary {
            count: num("count") as usize,
            start: num("start"),
            end: num("end"),
            mythic: num("mythic"),
        })
    }

    pub fn scanned(&self) -> Result<String, String> {
        let pending: bool = self.eval("__pending()")?;
        self.ask("collect-fights", |m| {
            m.set("pushFightIfNeeded", pending)?;
            m.set("scanningOnly", true)
        })?;
        self.ask("collect-scanned-raids", |_| Ok(()))?;
        self.eval("__scanned()")
    }

    pub fn stamps(&self, list: &[(&str, i64)]) -> Result<Vec<i64>, String> {
        let input = serde_json::to_string(list).map_err(|e| e.to_string())?;
        let output: String = self.eval(&format!("__stamps({input})"))?;
        let times: Vec<Option<f64>> = serde_json::from_str(&output).map_err(|e| e.to_string())?;
        times
            .into_iter()
            .map(|t| {
                t.map(|t| t as i64)
                    .ok_or_else(|| "the parser cannot read a log timestamp".to_owned())
            })
            .collect()
    }

    pub fn fights_text(&self) -> Result<String, String> {
        self.eval("__fightsText()")
    }

    pub fn master(&self, code: &str) -> Result<(String, String), String> {
        self.ask("collect-master-info", |m| m.set("reportCode", code))?;

        let error: String = self.eval("__masterError()")?;
        if !error.is_empty() {
            return Err(error);
        }

        Ok((self.eval("__masterText()")?, self.eval("__players()")?))
    }

    pub fn clear(&self) -> Result<(), String> {
        self.ask("clear-fights", |_| Ok(()))?;
        self.eval::<()>("__release()")?;
        self.rt.run_gc();
        Ok(())
    }
}
