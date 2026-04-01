# coding=utf8
import datetime
import json

statistics = [
    "select id, item_id, reply_id, data, datetime(create_time, '+8 hours') from blob_storage where item_id > 0",
    "select url, counter, datetime(update_time, '+8 hours') from item where counter > 0 order by update_time desc"
]


def from_grafana_alert_item(v):
    return f"""<entry>
        <title>{v["status"].upper()}: {v["labels"]["alertname"]} - {v["labels"]["filter_group"]} </title>
        <id>{v["fingerprint"]}.{int(datetime.datetime.fromisoformat(v["startsAt"]).timestamp())}.{v["status"]}</id>
        <updated>{v["startsAt"]}</updated>
        <summary>{"\n".join(f"{k}: {v}" for k, v in v["values"].items())}</summary>
        <link href="{v["silenceURL"]}" rel="alternate"/>
    </entry>"""


def from_grafana_alert(body):
    value = json.loads(body)
    return f"""<?xml version="1.0" encoding="utf-8"?>
    <feed xmlns="http://www.w3.org/2005/Atom">
        <title>From Grafana Alert</title>
        <id>https://example.com/feed.atom</id>
        <updated>{datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")}</updated>
        <author>
            <name>Your Name or Organization</name>
            <email>your.email@example.com</email>
        </author>
    <link href="https://example.com" rel="alternate"/>
    {"\n".join(from_grafana_alert_item(v) for v in value["alerts"])}
</feed>"""
