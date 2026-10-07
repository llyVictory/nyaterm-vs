// Docker network changes can invalidate Chromium's first module requests.
// Retry only that transport failure, before any login or application actions.
export async function gotoLogin(page, url) {
  for (let attempt = 0; attempt < 3; attempt++) {
    let networkChanged = false;
    const pageErrors = [];
    const onRequestFailed = (request) => {
      if (request.failure()?.errorText === "net::ERR_NETWORK_CHANGED")
        networkChanged = true;
    };
    const onPageError = (error) => pageErrors.push(error);
    page.on("requestfailed", onRequestFailed);
    page.on("pageerror", onPageError);
    try {
      await page.goto(url);
      await page.locator("#web-password").waitFor({
        state: "visible",
        timeout: 10_000,
      });
      if (pageErrors.length)
        throw new AggregateError(pageErrors, "Errors during login navigation");
      return;
    } catch (error) {
      networkChanged ||= String(error).includes("net::ERR_NETWORK_CHANGED");
      const unexpectedError = pageErrors.some(
        (entry) =>
          !String(entry).includes(
            "Failed to fetch dynamically imported module",
          ),
      );
      if (!networkChanged || unexpectedError || attempt === 2) throw error;
      console.warn(
        `Retrying login navigation after ERR_NETWORK_CHANGED (${attempt + 1}/2)`,
      );
    } finally {
      page.off("requestfailed", onRequestFailed);
      page.off("pageerror", onPageError);
    }
  }
}
