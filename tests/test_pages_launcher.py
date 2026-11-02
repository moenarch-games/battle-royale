"""Keep the published Pages entry aimed at the shared, serverless MMORPG practice mode."""

from html.parser import HTMLParser
from pathlib import Path
import unittest


TARGET = "https://moenarch-games.github.io/mmorpg/?activity=battle-royale"
ENTRY = Path(__file__).resolve().parents[1] / "pages" / "index.html"


class LauncherParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []
        self.active_href = None
        self.scripts = 0

    def handle_starttag(self, tag, attrs):
        if tag == "a":
            self.active_href = dict(attrs).get("href")
            self.links.append([self.active_href, ""])
        elif tag == "script":
            self.scripts += 1

    def handle_data(self, data):
        if self.active_href is not None:
            self.links[-1][1] += data

    def handle_endtag(self, tag):
        if tag == "a":
            self.active_href = None


class PagesLauncherTest(unittest.TestCase):
    def test_practice_uses_shared_pages_client_without_a_host(self):
        page = ENTRY.read_text(encoding="utf-8")
        parser = LauncherParser()
        parser.feed(page)

        self.assertIn('name="viewport"', page)
        self.assertEqual(parser.links, [[TARGET, "Play local practice"]])
        self.assertNotIn("arenaServer=", page)
        self.assertEqual(parser.scripts, 0)


if __name__ == "__main__":
    unittest.main()
