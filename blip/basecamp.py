"""Basecamp: which of my to-dos are overdue or due today, and ticking them off.

Two ways to connect (Settings > Basecamp):
  * paste a Basecamp personal access token, or
  * OAuth with a Launchpad integration (client id + secret, redirect
    http://localhost:47615/callback) - Blip opens the browser and catches the reply.
"""
import http.server
import json
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import webbrowser

UA = "Blip desktop pet (https://github.com/safder2000/blip)"
LAUNCHPAD = "https://launchpad.37signals.com"
API_AUTH = "https://3.basecampapi.com/authorization.json"
REDIRECT_PORT = 47615
REDIRECT = f"http://localhost:{REDIRECT_PORT}/callback"
SCOPES = ("overdue", "due_today")

try:  # filled in by CI from repository secrets, so release builds can offer one-click OAuth
    from ._build import BC_CLIENT_ID as _CID, BC_CLIENT_SECRET as _CSECRET
except ImportError:
    _CID = _CSECRET = ""


class BasecampError(Exception):
    pass


def api(method, url, token=None, form=None, retries=2, timeout=20):
    headers = {"User-Agent": UA, "Accept": "application/json"}
    data = None
    if token:
        headers["Authorization"] = "Bearer " + token
    if form is not None:
        data = urllib.parse.urlencode(form).encode()
        headers["Content-Type"] = "application/x-www-form-urlencoded"
    elif method == "POST":
        data = b""
    for attempt in range(retries + 1):
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                body = r.read()
                return r.status, (json.loads(body) if body.strip() else None)
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504) and attempt < retries:
                time.sleep(min(30, int(e.headers.get("Retry-After") or 2 ** (attempt + 1))))
                continue
            if e.code == 401:
                raise BasecampError("Basecamp says the token is not valid any more - reconnect in Settings.")
            raise BasecampError(f"Basecamp error {e.code}")
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            if attempt < retries:
                time.sleep(2)
                continue
            raise BasecampError(f"Can't reach Basecamp ({getattr(e, 'reason', e)})")
    raise BasecampError("Basecamp request failed")


def pick_accounts(auth_doc):
    return [{"name": a.get("name", "?"), "href": a["href"]}
            for a in (auth_doc or {}).get("accounts", []) if a.get("product") == "bc3" and a.get("href")]


def normalise(items, scope):
    out = []
    for it in items or []:
        out.append({
            "id": it.get("id"),
            "type": it.get("type", "todo"),
            "content": (it.get("content") or it.get("title") or "").strip(),
            "due_on": it.get("due_on"),
            "project": (it.get("bucket") or {}).get("name", ""),
            "bucket_id": (it.get("bucket") or {}).get("id"),
            "url": it.get("app_url", ""),
            "scope": scope,
        })
    return out


class Basecamp:
    def __init__(self, cfg, post):
        self.cfg = cfg
        self.post = post          # post(fn, *args) runs fn on the UI thread
        self.items = []
        self.last_error = None
        self.last_fetch = 0.0
        self._server = None

    # ------------------------------------------------ connection
    @property
    def connected(self):
        return bool(self.cfg.get("bc_token") and self.cfg.get("bc_account"))

    def client_creds(self):
        return (self.cfg.get("bc_client_id") or _CID, self.cfg.get("bc_client_secret") or _CSECRET)

    def disconnect(self):
        self.cfg.update(bc_token="", bc_refresh="", bc_expires=0, bc_account="", bc_account_name="")
        self.cfg.save()
        self.items = []

    def _finish_connect(self, token, refresh="", expires_in=0):
        _, doc = api("GET", API_AUTH, token)
        accts = pick_accounts(doc)
        if not accts:
            raise BasecampError("That login has no Basecamp account.")
        keep = next((a for a in accts if a["href"] == self.cfg.get("bc_account")), accts[0])
        self.cfg.update(bc_token=token, bc_refresh=refresh,
                        bc_expires=(time.time() + expires_in) if expires_in else 0,
                        bc_account=keep["href"], bc_account_name=keep["name"])
        self.cfg.save()
        return accts

    def connect_token(self, token, done):
        """Personal access token. done(ok, message, accounts) on the UI thread."""
        def work():
            try:
                accts = self._finish_connect(token.strip())
                self.post(done, True, f"Connected to {self.cfg['bc_account_name']}", accts)
                self.refresh()
            except BasecampError as e:
                self.post(done, False, str(e), [])
        threading.Thread(target=work, daemon=True).start()

    def connect_oauth(self, done):
        cid, secret = self.client_creds()
        if not (cid and secret):
            done(False, "Add the integration's client ID and secret first.", [])
            return
        bc = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_GET(self):
                q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
                code = (q.get("code") or [""])[0]
                ok_page = code and self.path.startswith("/callback")
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.end_headers()
                msg = "Blip is connected to Basecamp. You can close this tab." if ok_page \
                    else "Basecamp didn't send a code. Try again from Blip's settings."
                self.wfile.write(f"<html><body style='font:16px sans-serif;padding:40px'>{msg}</body></html>".encode())
                if ok_page:
                    threading.Thread(target=bc._exchange, args=(code, done), daemon=True).start()
                    threading.Thread(target=bc._stop_server, daemon=True).start()

        try:
            self._stop_server()
            self._server = http.server.HTTPServer(("127.0.0.1", REDIRECT_PORT), Handler)
        except OSError as e:
            done(False, f"Port {REDIRECT_PORT} is busy ({e}).", [])
            return
        threading.Thread(target=self._server.serve_forever, daemon=True).start()
        threading.Timer(600, self._stop_server).start()
        webbrowser.open(f"{LAUNCHPAD}/authorization/new?" + urllib.parse.urlencode(
            {"response_type": "code", "client_id": cid, "redirect_uri": REDIRECT}))

    def _stop_server(self):
        s, self._server = self._server, None
        if s:
            try:
                s.shutdown()
                s.server_close()
            except Exception:
                pass

    def _exchange(self, code, done):
        cid, secret = self.client_creds()
        try:
            _, tok = api("POST", f"{LAUNCHPAD}/authorization/token", form={
                "grant_type": "authorization_code", "client_id": cid,
                "client_secret": secret, "redirect_uri": REDIRECT, "code": code})
            accts = self._finish_connect(tok["access_token"], tok.get("refresh_token", ""),
                                         int(tok.get("expires_in") or 0))
            self.post(done, True, f"Connected to {self.cfg['bc_account_name']}", accts)
            self.refresh()
        except (BasecampError, KeyError, TypeError) as e:
            self.post(done, False, f"Couldn't finish connecting: {e}", [])

    def _refresh_token_if_needed(self):
        exp, refresh = self.cfg.get("bc_expires") or 0, self.cfg.get("bc_refresh")
        if not refresh or not exp or exp - time.time() > 2 * 86400:
            return
        cid, secret = self.client_creds()
        _, tok = api("POST", f"{LAUNCHPAD}/authorization/token", form={
            "grant_type": "refresh_token", "refresh_token": refresh,
            "client_id": cid, "client_secret": secret, "redirect_uri": REDIRECT})
        self.cfg.update(bc_token=tok["access_token"], bc_refresh=tok.get("refresh_token", refresh),
                        bc_expires=time.time() + int(tok.get("expires_in") or 1209600))
        self.cfg.save()

    def set_account(self, href, name):
        self.cfg.update(bc_account=href, bc_account_name=name)
        self.cfg.save()
        self.refresh()

    # ------------------------------------------------ data
    def fetch_due(self):
        self._refresh_token_if_needed()
        base, tok = self.cfg["bc_account"].rstrip("/"), self.cfg["bc_token"]
        out = []
        for scope in SCOPES:
            _, items = api("GET", f"{base}/my/assignments/due.json?scope={scope}", tok)
            out += normalise(items, scope)
        return out

    def refresh(self, done=None):
        """Fetch in the background; results go to self.on_items / done on the UI thread."""
        if not self.connected:
            return

        def work():
            try:
                items = self.fetch_due()
                self.items, self.last_error, self.last_fetch = items, None, time.time()
            except BasecampError as e:
                self.last_error = str(e)
            self.post(self.on_items, self.items, self.last_error)
            if done:
                self.post(done)
        threading.Thread(target=work, daemon=True).start()

    def on_items(self, items, error):   # replaced by the app
        pass

    def complete(self, item, done):
        def work():
            try:
                base, tok = self.cfg["bc_account"].rstrip("/"), self.cfg["bc_token"]
                api("POST", f"{base}/todos/{item['id']}/completion.json", tok)
                self.items = [i for i in self.items if i["id"] != item["id"]]
                self.post(done, True, "")
            except BasecampError as e:
                self.post(done, False, str(e))
        threading.Thread(target=work, daemon=True).start()
