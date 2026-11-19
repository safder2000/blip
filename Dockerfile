# Blip server (see SERVER.md). Build straight from GitHub:
#   docker build -t blip-server https://github.com/safder2000/blip.git
#   docker run -d --name blip-server --restart unless-stopped -p 47620:47620 blip-server
FROM python:3.12-alpine
WORKDIR /app
COPY blip/server.py blip/ws.py ./
ENV PORT=47620
EXPOSE 47620
HEALTHCHECK --interval=30s --timeout=5s CMD python -c "import os,urllib.request; urllib.request.urlopen('http://127.0.0.1:'+os.environ['PORT']+'/health', timeout=4)"
CMD ["python", "-u", "server.py"]
