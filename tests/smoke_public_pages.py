"""CDN acceptance for Battle Royale Pages and the shared MMORPG practice arena.

Run with Playwright/Chromium, using SwiftShader to exercise the real WebGL client
without requiring a runner GPU. This intentionally tests the public Pages URLs.
"""

from pathlib import Path
import re

from playwright.sync_api import expect, sync_playwright


ENTRY = "https://moenarch-games.github.io/battle-royale/"
GAME = "https://moenarch-games.github.io/mmorpg/?activity=battle-royale"
EVIDENCE = Path("artifacts/pages-smoke")


def walk_with_touch(context, page):
    session = context.new_cdp_session(page)
    try:
        point = lambda y: [{"x": 60, "y": y, "id": 1}]
        session.send("Input.dispatchTouchEvent", {"type": "touchStart", "touchPoints": point(720)})
        for y in (706, 692, 678, 664, 650):
            session.send("Input.dispatchTouchEvent", {"type": "touchMove", "touchPoints": point(y)})
        page.wait_for_timeout(5_000)
        session.send("Input.dispatchTouchEvent", {"type": "touchEnd", "touchPoints": []})
    finally:
        session.detach()


def check(browser, name, mobile):
    context = browser.new_context(
        viewport={"width": 390, "height": 844} if mobile else {"width": 1440, "height": 900},
        is_mobile=mobile,
        has_touch=mobile,
        device_scale_factor=1,
        reduced_motion="reduce",
    )
    page = context.new_page()
    page_errors = []
    console_errors = []
    bad_assets = []
    loaded_assets = []
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    page.on("console", lambda message: console_errors.append(message.text) if message.type == "error" else None)

    def record_response(response):
        if not response.url.startswith("https://moenarch-games.github.io/mmorpg/"):
            return
        if response.status >= 400:
            bad_assets.append(f"{response.status} {response.url}")
        elif response.status < 300:
            loaded_assets.append(response.url)

    page.on("response", record_response)
    try:
        response = page.goto(ENTRY, wait_until="domcontentloaded", timeout=30_000)
        assert response and response.status == 200, f"{ENTRY} returned {response.status if response else 'no response'}"
        play = page.get_by_role("link", name="Play local practice")
        expect(play).to_be_visible()
        if mobile:
            play.tap()
        else:
            play.click()
        expect(page).to_have_url(GAME)

        start = page.get_by_role("button", name=re.compile("^Start practice"))
        expect(start).to_be_enabled(timeout=60_000)
        if mobile:
            start.tap()
        else:
            start.click()
        expect(page.locator("#character-select")).to_be_hidden()
        expect(page.locator("#unit-status")).to_contain_text("Level 10")

        if mobile:
            expect(page.get_by_role("button", name="Target", exact=True)).to_be_visible()
            expect(page.get_by_role("button", name="Attack", exact=True)).to_be_visible()
            walk_with_touch(context, page)
            page.get_by_role("button", name="Target", exact=True).tap()
        else:
            page.locator("#world").focus()
            page.keyboard.down("KeyW")
            page.wait_for_timeout(5_000)
            page.keyboard.up("KeyW")
            page.keyboard.press("Tab")

        expect(page.locator("#unit-status")).to_contain_text("Target: Training Target")
        page.screenshot(path=str(EVIDENCE / f"{name}.png"), full_page=True)
        assert any(".wasm" in url for url in loaded_assets), "MMORPG WASM asset did not load"
        assert any("/assets/" in url and ".js" in url for url in loaded_assets), "MMORPG JS assets did not load"
        assert not bad_assets, f"Failed MMORPG assets: {bad_assets}"
        assert not page_errors, f"JavaScript exceptions: {page_errors}"
        assert not console_errors, f"Browser console errors: {console_errors}"
        print(f"{name}: entry, practice, movement, targeting, assets and console passed")
    finally:
        context.close()


if __name__ == "__main__":
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader"])
        try:
            check(browser, "desktop", mobile=False)
            check(browser, "mobile", mobile=True)
        finally:
            browser.close()
