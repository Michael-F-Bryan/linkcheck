use crate::validation::{CacheEntry, Context, Reason};
use http::HeaderMap;
use reqwest::{Client, Url};
use std::time::SystemTime;

#[deprecated]
/// Send a HEAD request to a particular endpoint.
///
/// This function is deprecated in favor of [`head`].
pub async fn get(
    client: &Client,
    url: Url,
    extra_headers: HeaderMap,
) -> Result<(), reqwest::Error> {
    head(client, url, extra_headers).await?;
    Ok(())
}

/// Send a HEAD request to a particular endpoint.
pub async fn head(
    client: &Client,
    url: Url,
    extra_headers: HeaderMap,
) -> Result<(), reqwest::Error> {
    client
        .head(url)
        .headers(extra_headers)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

/// Check whether a [`Url`] points to a valid resource on the internet.
pub async fn check_web<C>(url: &Url, ctx: &C) -> Result<(), Reason>
where
    C: Context + ?Sized,
{
    log::debug!("Checking \"{}\" on the web", url);

    if already_valid(&url, ctx) {
        log::debug!("The cache says \"{}\" is still valid", url);
        return Ok(());
    }

    let result =
        head(ctx.client(), url.clone(), ctx.url_specific_headers(&url)).await;

    if let Some(fragment) = url.fragment() {
        // TODO: check the fragment
        log::warn!("Fragment checking isn't implemented, not checking if there is a \"{}\" header in \"{}\"", fragment, url);
    }

    let entry = CacheEntry::new(SystemTime::now(), result.is_ok());
    update_cache(url, ctx, entry);

    result.map_err(Reason::from)
}

fn already_valid<C>(url: &Url, ctx: &C) -> bool
where
    C: Context + ?Sized,
{
    if let Some(cache) = ctx.cache() {
        return cache.url_is_still_valid(url, ctx.cache_timeout());
    }

    false
}

fn update_cache<C>(url: &Url, ctx: &C, entry: CacheEntry)
where
    C: Context + ?Sized,
{
    if let Some(mut cache) = ctx.cache() {
        cache.insert(url.clone(), entry);
    }
}
