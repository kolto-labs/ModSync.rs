import contextlib
import io
import tempfile
import unittest
from pathlib import Path

import site_check

ROOT = Path(__file__).resolve().parents[2]


class SiteCheck(unittest.TestCase):
    def site(self, files):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        for name, body in files.items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(body, encoding="utf-8")
        return root

    def problems(self, files):
        return site_check.broken_links(self.site(files))[1]

    def test_a_site_whose_local_links_all_resolve_passes(self):
        self.assertEqual(self.problems({
            "index.html": '<a href="faq.html#top">x</a><img src="assets/logo.svg">'
                          '<a href="https://example.com/missing">ext</a><a href="#here">frag</a>'
                          '<a href="mailto:a@b.c">m</a><a href="docs/">dir</a>',
            "faq.html": "<p>faq</p>", "assets/logo.svg": "<svg/>", "docs/index.html": "<p>d</p>"}), [])

    def test_a_missing_local_target_fails(self):
        self.assertEqual(self.problems({"index.html": '<a href="gone.html">x</a>'}),
                         ["index.html: gone.html does not exist"])

    def test_a_link_escaping_the_site_fails(self):
        self.assertEqual(self.problems({"index.html": '<a href="../secret.html">x</a>'}),
                         ["index.html: ../secret.html points outside the site"])

    def test_a_site_without_an_entry_page_fails(self):
        self.assertIn("index.html is missing", self.problems({"other.html": "<p>x</p>"}))

    def test_an_empty_site_fails(self):
        self.assertIn("no HTML pages found", self.problems({}))

    def test_main_exits_nonzero_on_problems_and_zero_on_the_real_site(self):
        broken = self.site({"index.html": '<img src="nope.png">'})
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(site_check.main([str(broken)]), 1)
            self.assertEqual(site_check.main([str(ROOT / "site")]), 0)


if __name__ == "__main__":
    unittest.main()
