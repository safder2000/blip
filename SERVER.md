# Blip server

Out of the box, Blip pets only find each other on the same office network. A Blip server lets
pets talk from anywhere: home, another office, another city. That includes messages, calls and
voice calls.

You don't need one to use Blip. It's an optional extra for teams that aren't all in one place.

## How it works

```
 your PC ──┐                         ┌── teammate's PC
           ├── wss ──▶ Blip server ◀── wss ──┤
 laptop ───┘                         └── home PC
```

- Every pet keeps one connection open to the server, and the server passes messages between pets on the same team.
- **End-to-end encrypted.** Each pet encrypts every message and every bit of call audio with the team key before sending,
  so the server only passes along scrambled data and sees just a team ID and message sizes.
  On the shared team the key comes from the server's address, which keeps out anyone sniffing the network.
  For privacy from whoever runs the server too, use a private team link: its code (the part after `#`) never
  reaches the server.
- **Teams are separate.** Different invite links mean different teams. Pets on one team never see pets on another,
  even on the same server.
- **Voice** is compressed with Opus, about 20 kbps per person. A small server can carry many calls at once.
- **On the office network**, pets still talk to each other directly, as they do without a server.

## 1. Run the server

Pick one. The Docker image and the Linux binary are the Rust server, a single small program;
the Python one (`server.py`) works the same way and is there for anywhere Python is easier.

**Docker**

```
docker build -t blip-server https://github.com/safder2000/blip.git
docker run -d --name blip-server --restart unless-stopped -p 47620:47620 blip-server
```

**Docker Compose** (TrueNAS SCALE custom app, Synology Container Manager, Unraid, Portainer...)

```yaml
services:
  blip:
    build: https://github.com/safder2000/blip.git
    restart: unless-stopped
    ports:
      - "47620:47620"
```

**Linux binary** (x86-64 Linux, e.g. a NAS or VPS; on a Raspberry Pi use Docker or Python)

```
curl -fsSLO https://github.com/safder2000/blip/releases/latest/download/blip-server-linux-x64
chmod +x blip-server-linux-x64 && ./blip-server-linux-x64
```

**Plain Python** (any PC or Raspberry Pi with Python 3.8+)

```
curl -O https://raw.githubusercontent.com/safder2000/blip/main/blip/server.py
curl -O https://raw.githubusercontent.com/safder2000/blip/main/blip/ws.py
python3 server.py
```

**From Blip's source:** `python -m blip.server` (the old name `python -m blip.relay` still works).

Check it: open `http://<server-ip>:47620` in a browser. You should see *Blip server is running*.

Options: `--port 8080` picks another port. On platforms that set a `$PORT` environment variable, the server uses it automatically.

## 2. Make it reachable

| Who needs to connect | What to do | Address for Blip |
|---|---|---|
| Only people on your network | Nothing | `192.168.1.200:47620` |
| Anyone, no ports opened (recommended) | Cloudflare Tunnel | `https://blip.example.com` |
| Anyone, you have a public/static IP | Forward TCP 47620 on the router | `http://203.0.113.5:47620` |
| Anyone, VPS or cloud | Run it there | `https://blip.example.com` |

### Cloudflare Tunnel (free, no open ports, free HTTPS)

1. Install `cloudflared` on the server machine, or reuse a tunnel you already run.
   Cloudflare dashboard → **Zero Trust → Networks → Tunnels → Create a tunnel** walks you through it.
2. In the tunnel, add a **Public hostname**:
   - Subdomain `blip`, your domain
   - Service type **HTTP**, URL `localhost:47620` (or `<server-ip>:47620` if cloudflared runs elsewhere)
3. Open `https://blip.yourdomain.com`. It should show the status page.

WebSockets are on by default in Cloudflare, and nothing else needs changing.

Just to try it out without an account: `cloudflared tunnel --url http://localhost:47620` prints a temporary `https://….trycloudflare.com` address.

### Port forwarding (static or public IP)

Forward **TCP 47620** on the router to the server machine, then use `http://<public-ip>:47620`.
Messages are already end-to-end encrypted, but HTTPS also hides who talks to whom.
For that, put Caddy in front, which gets a certificate on its own:

```
blip.example.com {
    reverse_proxy localhost:47620
}
```

### VPS (DigitalOcean, Hetzner, Lightsail...)

Run the Docker command above, then add Caddy as shown, or a Cloudflare Tunnel.
The smallest VPS size is plenty.

### Render, Railway, Fly.io

Create a web service from this repository. They use the `Dockerfile` and set `$PORT`, which the server picks up.
The address they give you (for example `https://blip-xxxx.onrender.com`) is the one to paste into Blip.

Render's free plan sleeps after 15 minutes without visitors, and the first pet to connect wakes it, which takes about a minute.
Pets reconnect on their own, but an always-on plan works better for calls.

## 3. Connect the pets

Everyone enters the server's address in **Blip → Settings → Office → Online server**, for example
`https://blip.example.com` or `192.168.1.200:47620`. That's all: everyone who enters the same address is on one
shared team and sees each other. **Copy link** in that panel gives you the address to send around.

**Private teams:** for a group that should stay separate from everyone else on the server, open the server's
address in a browser and click **Make a private team link**. You get something like

```
https://blip.example.com/#team=3f9c2a7d41b0e85c96aa12f0
```

and only people who paste that exact link see each other. Settings shows which team you're on, and
*Join the shared team instead* takes you back.

The status line under the field shows *Connected to … · N teammates online*.
Teammates appear in the right-click menu with a small globe, and the same call and message buttons.
If both PCs have a microphone, calls get a **Talk** button. Use headsets, since there's no echo cancelling.

## Good to know

- **Who can join:** anyone who knows the address can join the shared team. Fine for a company server; for a
  public one, use private team links. Anyone with a private link can join that team, so share it like a password.
  To remove someone, make a new private link and give it only to the people who should stay.
- **The server stores nothing:** no accounts, no history, no logs of messages.
  Restart it whenever you like; pets reconnect within a few seconds.
- **Health check:** `GET /health` returns JSON with how many pets and teams are connected.
- **Limits:** 5000 connections, 1000 pets per team, and a per-connection rate limit high enough for voice.
- **Pets on the office network** keep talking to each other directly even when a server is set,
  and they use the server only for teammates they can't reach directly.

## Troubleshooting

| Status in Blip | Meaning |
|---|---|
| *Can't connect: can't find that address* | Typo in the domain, or the DNS record doesn't exist yet |
| *Can't connect: nothing is listening there* | Server not running, wrong port, or firewall/router not forwarding |
| *Can't connect: not a Blip server* | The address answers, but with something else (wrong port, or the proxy isn't pointing at the server) |
| *Connected · 0 teammates online* | Server works. Teammates have a different team link, or haven't connected yet |
