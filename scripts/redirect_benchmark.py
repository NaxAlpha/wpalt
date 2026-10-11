#!/usr/bin/env python3
"""Real serial route/report observations; not a throughput or peak-memory test."""
import argparse
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def run(binary, postgres, psql):
    observations = []
    for engine in (["sqlite", "postgres"] if postgres else ["sqlite"]):
        with tempfile.TemporaryDirectory(prefix="wpalt-route-measure-") as temporary:
            root = Path(temporary)
            schema = "wpalt_route_measure_" + uuid.uuid4().hex
            server = log = None

            def pg(statement):
                return subprocess.check_output(
                    [psql, postgres, "-X", "-qAt", "-v", "ON_ERROR_STOP=1", "-c", statement],
                    text=True,
                ).strip()

            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                port = listener.getsockname()[1]
            origin = f"http://127.0.0.1:{port}"
            url = "sqlite://" + str(root / "site.db") + "?mode=rwc"
            if engine == "postgres":
                pg("CREATE SCHEMA " + schema)
                parts = urllib.parse.urlsplit(postgres)
                query = urllib.parse.parse_qsl(parts.query) + [("options", "-c search_path=" + schema)]
                url = urllib.parse.urlunsplit(parts._replace(query=urllib.parse.urlencode(query)))
            config = root / "site.toml"
            config.write_text(
                f"database_url={json.dumps(url)}\ndata_dir={json.dumps(str(root / 'data'))}\n"
                f'listen="127.0.0.1:{port}"\nbase_url="{origin}"\ndebug=true\n'
            )
            password = secrets.token_urlsafe(24)

            def native(*argv, stdin=None):
                result = subprocess.run(
                    [str(binary), "--config", str(config), *argv], input=stdin,
                    text=True, capture_output=True, timeout=120,
                )
                assert result.returncode == 0, result.stderr.replace(password, "[redacted]")

            client = urllib.request.build_opener(
                urllib.request.ProxyHandler({}),
                urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
            )

            visitor = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            csrf = None

            def request(path, data=None, form=False, anonymous=False):
                headers = {"Origin": origin}
                if csrf:
                    headers["X-CSRF-Token"] = csrf
                if data is not None:
                    headers["Content-Type"] = "application/x-www-form-urlencoded" if form else "application/json"
                    data = (urllib.parse.urlencode(data) if form else json.dumps(data)).encode()
                try:
                    response = (visitor if anonymous else client).open(urllib.request.Request(origin + path, data=data, headers=headers), timeout=30)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    return response.status, response.read()

            try:
                native("init", "--admin-email", "owner@example.test", stdin=password + "\n")
                native("seed-demo", "--posts", "150")
                log = (root / "server.log").open("w")
                server = subprocess.Popen([str(binary), "--config", str(config), "serve", "--external-worker"], stdout=log, stderr=log)
                for _ in range(100):
                    try:
                        request("/login")
                        break
                    except OSError:
                        time.sleep(.1)
                else:
                    raise RuntimeError("Fixture application not ready")
                status, _ = request("/login", {"email": "owner@example.test", "password": password}, True)
                assert status == 200
                _, html = request("/admin/builder")
                csrf = re.search(rb'data-csrf="([^"]+)"', html).group(1).decode()
                status, body = request("/api/admin/content", {"csrf": csrf, "title": "Route measurement", "slug": "route-measured", "kind": "post", "fields": "{}", "body": "Actual measured publication.", "action": "publish"})
                assert status == 200, body
                rules = [{"id": f"measured_{i}", "label": f"Measured rule {i}", "pattern": f"^/legacy-{i}/([a-z0-9-]+)$", "target": "/$1", "code": 301, "priority": i, "case_insensitive": False, "enabled": True} for i in range(128)]

                def configure(enabled):
                    _, body = request("/api/discovery/redirect-policy")
                    policy = json.loads(body)
                    settings = policy["policy"]["settings"]
                    settings.update(report_404=enabled, rules=rules)
                    started = time.perf_counter()
                    status, body = request("/api/discovery/redirect-policy", {"csrf": csrf, "version": policy["version"], "settings": settings})
                    assert status == 200, body
                    return (time.perf_counter() - started) * 1000

                def observe(path, expected):
                    samples = []
                    for index in range(35):
                        started = time.perf_counter()
                        status, body = request(path, anonymous=True)
                        assert status == expected
                        if index >= 5:
                            samples.append((time.perf_counter() - started) * 1000)
                    offset = (root / "server.log").stat().st_size
                    request(path, anonymous=True)
                    time.sleep(.03)
                    with (root / "server.log").open() as diagnostics:
                        diagnostics.seek(offset)
                        lines = diagnostics.read(1024 * 1024).splitlines()
                    shapes = []
                    for line in lines:
                        try:
                            event = json.loads(line)
                        except ValueError:
                            continue
                        fields = event.get("fields", {})
                        statement = fields.get("db.statement") or fields.get("summary", "")
                        if event.get("target", "").startswith("sqlx") and statement.strip().upper().startswith(("SELECT", "UPDATE", "INSERT", "DELETE")):
                            shapes.append(fields.get("summary", statement.splitlines()[0])[:200])
                    rss = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(server.pid)], text=True).strip()) * 1024
                    return {"samples": 30, "p50_ms": round(statistics.median(samples), 3), "p95_ms": round(sorted(samples)[28], 3), "response_bytes": len(body), "observed_sql": len(shapes), "sql_shapes": shapes, "warm_rss_bytes": rss}

                timings = [configure(False) for _ in range(3)]
                disabled = observe("/missing-measured", 404)
                # urllib follows the real 301, so this includes the destination render.
                capture = observe("/legacy-127/route-measured", 200)
                configure(True)
                enabled = observe("/missing-measured", 404)
                started = time.perf_counter()
                status, graph = request("/admin/discovery/rendered-links", {"csrf": csrf, "output": "json"}, True)
                assert status == 200, graph[:300]
                graph = json.loads(graph)
                assert graph["complete"] and len(graph["sources"]) == 153
                graph_observation = {"request_ms": round((time.perf_counter() - started) * 1000, 3), "rendered_ms": round(graph["elapsed_ms"], 3), "sources": len(graph["sources"]), "edges": len(graph["edges"]), "html_bytes": graph["html_bytes"]}
                # Independent oversized synthetic DB fixture; not a publication/API claim.
                if engine == "sqlite":
                    with sqlite3.connect(root / "site.db") as connection:
                        columns = [row[1] for row in connection.execute("PRAGMA table_info(posts)")]
                        expressions = ["'overflow-'||n" if name in ("id", "slug", "published_slug") else "''" if name in ("translation_group", "published_translation_group") else 'p."' + name + '"' for name in columns]
                        connection.execute("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<851) INSERT INTO posts(" + ','.join('"'+name+'"' for name in columns) + ") SELECT " + ','.join(expressions) + " FROM (SELECT * FROM posts LIMIT 1) p CROSS JOIN numbers")
                else:
                    columns = pg("SELECT column_name FROM information_schema.columns WHERE table_schema='"+schema+"' AND table_name='posts' ORDER BY ordinal_position").splitlines()
                    expressions = ["'overflow-'||n::text" if name in ("id", "slug", "published_slug") else "''" if name in ("translation_group", "published_translation_group") else 'p."' + name + '"' for name in columns]
                    pg("INSERT INTO "+schema+".posts(" + ','.join('"'+name+'"' for name in columns) + ") SELECT " + ','.join(expressions) + " FROM (SELECT * FROM "+schema+".posts LIMIT 1) p CROSS JOIN generate_series(1,851) n")
                started = time.perf_counter()
                status, body = request("/admin/discovery/rendered-links", {"csrf": csrf, "output": "json"}, True)
                assert status == 422 and b"no partial orphan" in body
                refusal_ms = round((time.perf_counter() - started) * 1000, 3)
                queries = {
                    "publication_point": "SELECT id FROM posts WHERE published_slug='route-measured' AND published_slug<>'' AND status='published'",
                    "retention_range": "DELETE FROM missing_paths WHERE last_seen<0",
                    "exact_ordered_cap": "SELECT source,target,code FROM redirects ORDER BY source LIMIT 1001",
                    "policy_point": "SELECT definition FROM redirect_policy WHERE id=1",
                }
                if engine == "sqlite":
                    with sqlite3.connect(root / "site.db") as connection:
                        plans = {name: [row[3] for row in connection.execute("EXPLAIN QUERY PLAN " + query)] for name, query in queries.items()}
                else:
                    plans = {name: json.loads(pg("SET search_path=" + schema + "; EXPLAIN (FORMAT JSON) " + query)) for name, query in queries.items()}
                    plans["publication_forced_generic"] = json.loads(pg("SET search_path="+schema+"; SET plan_cache_mode=force_generic_plan; PREPARE measured_route(text) AS SELECT id FROM posts WHERE published_slug=$1 AND published_slug<>'' AND status='published'; EXPLAIN (FORMAT JSON) EXECUTE measured_route('route-measured')"))
                database_bytes = sum(file.stat().st_size for file in root.glob("site.db*")) if engine == "sqlite" else int(pg("SELECT SUM(pg_total_relation_size(quote_ident(schemaname)||'.'||quote_ident(tablename))) FROM pg_tables WHERE schemaname='" + schema + "'"))
                observations.append({"engine": engine, "rules": 128, "configuration_ms": [round(value, 3) for value in timings], "report_disabled": disabled, "report_enabled": enabled, "last_capture_with_destination": capture, "graph": graph_observation, "over_1000_publications_refusal_ms": refusal_ms, "database_bytes_after_larger_fixture": database_bytes, "query_plans": plans})
            finally:
                if server:
                    server.terminate()
                    server.wait(timeout=10)
                if log:
                    log.close()
                if engine == "postgres":
                    pg("DROP SCHEMA " + schema + " CASCADE")
    return observations


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--profile", choices=["debug", "release"], required=True)
    parser.add_argument("--postgres", default=os.environ.get("TEST_DATABASE_URL"))
    parser.add_argument("--psql", default=shutil.which("psql"))
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    binary = Path(args.binary).resolve()
    report = {"profile": args.profile, "benchmark_script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "binary_bytes": binary.stat().st_size, "scope": "Synthetic actual API publications, 128 regex rules, 150 seeded stories, the seeded about page, and one real publication. Debug SQL logging on; five warmups and 30 serial anonymous loopback requests; capture observations include the followed destination. Graph scanned 153 sources, then refuses an independent directly inserted >1000-publication DB fixture. RSS is warm application process memory, not peak or database-engine memory; database storage scopes differ. No throughput, CWV or relative WordPress efficiency claim.", "observations": run(binary, args.postgres, args.psql)}
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"profile": args.profile, "output": str(output), "engines": len(report["observations"]) }))
