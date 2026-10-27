//! `dotsafe connect`, `connections`, `disconnect`: accounts at providers and
//! deploy targets. The admin token is stored as a shared secret.

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result, bail};
use dotsafe_core::{Binding, Connection, Record, Secret};
use dotsafe_integrations::{Kind, SERVICES, check_settings, service};
use zeroize::Zeroizing;

use crate::Ctx;

/// Parses repeated `--set key=value` options.
pub fn settings(pairs: &[String]) -> Result<BTreeMap<String, String>> {
    pairs
        .iter()
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_string(), v.trim().to_string())),
            _ => bail!("expected KEY=VALUE, got {pair:?}"),
        })
        .collect()
}

/// Every live connection.
pub fn connections(ctx: &Ctx) -> Result<Vec<Connection>> {
    let mut list: Vec<Connection> = ctx
        .vault
        .list()?
        .into_iter()
        .filter_map(|r| match r {
            Record::Connection(c) if !c.deleted => Some(c),
            _ => None,
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// A connection by id.
pub fn by_id(ctx: &Ctx, id: uuid::Uuid) -> Result<Connection> {
    match ctx.vault.get(id)? {
        Some(Record::Connection(c)) if !c.deleted => Ok(c),
        _ => bail!("the connection this secret used was removed"),
    }
}

/// A connection by name.
pub fn find(ctx: &Ctx, name: &str) -> Result<Connection> {
    let matches: Vec<Connection> = connections(ctx)?.into_iter().filter(|c| c.name == name).collect();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no connection named {name:?}; see `dotsafe connections`"),
        _ => bail!("more than one connection is named {name:?}"),
    }
}

pub fn connect(service_id: &str, name: Option<String>, set: &[String], token_stdin: bool) -> Result<()> {
    let Ok(svc) = service(service_id) else {
        let known: Vec<&str> = SERVICES.iter().map(|s| s.id).collect();
        bail!("dotsafe doesn't know {service_id:?}; it can connect to {}", known.join(", "));
    };
    let settings = settings(set)?;
    check_settings(svc.connection_fields, &settings).map_err(|e| {
        let fields: Vec<String> = svc.connection_fields.iter().map(|f| format!("--set {}=… ({})", f.key, f.hint)).collect();
        anyhow::anyhow!("{e}. {} takes: {}", svc.label, fields.join("; "))
    })?;
    let ctx = Ctx::open()?;
    let name = name.unwrap_or_else(|| svc.label.to_string());
    if connections(&ctx)?.iter().any(|c| c.name == name) {
        bail!("there's already a connection named {name:?}; pass --name");
    }

    eprintln!("{}", svc.token_help);
    let credential = if svc.needs_token {
        let token = read_token(&format!("{} admin token: ", svc.label), token_stdin)?;
        let credential = Secret::new(format!("{name} admin token"), token.as_str(), ctx.stamp());
        ctx.vault.put(Record::Secret(credential.clone()))?;
        Some(credential.id)
    } else {
        None
    };
    let connection = Connection::new(svc.id, &name, credential, settings, ctx.stamp());
    ctx.vault.put(Record::Connection(connection))?;
    let next = match svc.kind {
        Kind::Provider => format!("`dotsafe secret provider SECRET --connection {name:?}`"),
        Kind::Target => format!("`dotsafe secret target add SECRET --connection {name:?}`"),
    };
    let stored = if credential.is_some() { " The token is stored in the vault." } else { "" };
    eprintln!("Connected {name}.{stored} Next: {next}.");
    Ok(())
}

/// Replaces a connection's admin token, keeping the old one retired in its
/// history.
pub fn reconnect(name: &str, token_stdin: bool) -> Result<()> {
    let ctx = Ctx::open()?;
    let connection = find(&ctx, name)?;
    let token = read_token(&format!("New admin token for {name}: "), token_stdin)?;
    let Some(credential) = connection.credential else {
        bail!("{name} doesn't use a token");
    };
    let Some(Record::Secret(mut secret)) = ctx.vault.get(credential)? else {
        bail!("{name}'s stored token is missing; disconnect and connect again");
    };
    secret.rotate(token.as_str(), Some("Replaced".into()), ctx.stamp())?;
    secret.retire_previous(ctx.stamp());
    ctx.vault.put(Record::Secret(secret))?;
    eprintln!("{name} uses the new token now.");
    Ok(())
}

fn read_token(prompt: &str, from_stdin: bool) -> Result<Zeroizing<String>> {
    let token = if from_stdin {
        let mut token = Zeroizing::new(String::new());
        std::io::stdin().read_to_string(&mut token).context("reading the token from stdin")?;
        Zeroizing::new(token.trim().to_string())
    } else {
        // Not session::password: DOTSAFE_PASSWORD is the master password.
        Zeroizing::new(rpassword::prompt_password(prompt)?)
    };
    if token.is_empty() {
        bail!("no token given");
    }
    Ok(token)
}

pub fn list() -> Result<()> {
    let ctx = Ctx::open()?;
    let list = connections(&ctx)?;
    if list.is_empty() {
        eprintln!("No connections yet. Add one with `dotsafe connect <service>`: openai, cloudflare_token, vercel, cloudflare_workers.");
    }
    for c in list {
        let label = service(&c.service).map(|s| s.label).unwrap_or("unknown");
        let settings: Vec<String> = c.settings.iter().map(|(k, v)| format!("{k}={v}")).collect();
        println!("{}\t{label}\t{}", c.name, settings.join(" "));
    }
    Ok(())
}

pub fn disconnect(name: &str) -> Result<()> {
    let ctx = Ctx::open()?;
    let mut connection = find(&ctx, name)?;
    let used = ctx.vault.list()?.into_iter().any(|r| match r {
        Record::Binding(b) => {
            b.provider.as_ref().is_some_and(|p| p.connection == connection.id)
                || b.targets().any(|(_, t)| t.connection == connection.id)
        }
        _ => false,
    });
    if used {
        bail!("secrets still use {name}; remove it from them first (`dotsafe secret provider SECRET --none`, `dotsafe secret target remove`)");
    }
    connection.deleted = true;
    connection.updated = ctx.stamp();
    let credential = connection.credential;
    ctx.vault.put(Record::Connection(connection))?;
    if let Some(Record::Secret(mut token)) = credential.map(|id| ctx.vault.get(id)).transpose()?.flatten() {
        token.delete(ctx.stamp());
        ctx.vault.put(Record::Secret(token))?;
    }
    eprintln!("Disconnected {name} and deleted its stored token. Revoke the token at the service too.");
    Ok(())
}

/// The binding for a secret, or a new empty one.
pub fn binding(ctx: &Ctx, secret: uuid::Uuid) -> Result<Binding> {
    Ok(match ctx.vault.get(Binding::id_for(secret))? {
        Some(Record::Binding(b)) => b,
        _ => Binding::new(secret, ctx.stamp()),
    })
}
