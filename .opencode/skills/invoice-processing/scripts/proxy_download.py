#!/usr/bin/env python3
"""Download files from geo-blocked invoice platforms via the Shanghai proxy.

Some invoice platforms (e.g., 51fapiao) block requests from non-mainland China
IPs. This script routes downloads through a proxy server in Shanghai.

Usage:
    python3 proxy_download.py <url> <output_file>
    python3 proxy_download.py <url> <output_file> --proxy <proxy_url>

Examples:
    # Download 51fapiao viewer HTML
    python3 proxy_download.py "https://dlj.51fapiao.cn/dlj/v7/abc123" temp.html

    # Download PDF using extracted URL
    python3 proxy_download.py "https://dlj.51fapiao.cn/dlj/v7/downloadFile/abc123?signatureString=xyz" invoice.pdf

Output (JSON to stdout):
    {"success": true, "file": "invoice.pdf", "size": 50972, "content_type": "application/pdf"}
    {"success": false, "error": "Proxy returned 502"}
"""

from __future__ import annotations

import json
import sys
import urllib.parse
import urllib.request

# Default proxy URL (Shanghai instance)
DEFAULT_PROXY = "http://150.158.50.252:8765"


def download_via_proxy(url: str, output_file: str, proxy_url: str = DEFAULT_PROXY) -> dict:
    """Download a URL via the Shanghai proxy and save to output_file."""
    encoded_url = urllib.parse.quote(url, safe="")
    fetch_url = f"{proxy_url}/fetch?url={encoded_url}"

    try:
        req = urllib.request.Request(fetch_url)
        resp = urllib.request.urlopen(req, timeout=30)
        content = resp.read()
        content_type = resp.headers.get("Content-Type", "unknown")

        with open(output_file, "wb") as f:
            f.write(content)

        return {
            "success": True,
            "file": output_file,
            "size": len(content),
            "content_type": content_type,
        }
    except urllib.error.HTTPError as e:
        error_body = e.read().decode("utf-8", errors="ignore")[:200]
        return {
            "success": False,
            "error": f"Proxy returned HTTP {e.code}: {error_body}",
        }
    except Exception as e:
        return {
            "success": False,
            "error": str(e),
        }


def main():
    if len(sys.argv) < 3:
        print(json.dumps({
            "success": False,
            "error": "Usage: proxy_download.py <url> <output_file> [--proxy <proxy_url>]"
        }))
        sys.exit(1)

    url = sys.argv[1]
    output_file = sys.argv[2]
    proxy_url = DEFAULT_PROXY

    if "--proxy" in sys.argv:
        idx = sys.argv.index("--proxy")
        if idx + 1 < len(sys.argv):
            proxy_url = sys.argv[idx + 1]

    result = download_via_proxy(url, output_file, proxy_url)
    print(json.dumps(result, ensure_ascii=False))

    if not result["success"]:
        sys.exit(1)


if __name__ == "__main__":
    main()
