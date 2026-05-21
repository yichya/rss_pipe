# coding=utf8
import datetime
import json
import random

statistics = [
    "select id, item_id, reply_id, data, datetime(create_time, '+8 hours') from blob_storage where item_id > 0",
    "select url, counter, datetime(update_time, '+8 hours') from item where counter > 0 order by update_time desc",
    "select json_extract(value, '$.title'), json_array_length(value, '$.item'), expire_at - unixepoch() from redis_storage where value like '%bilibili%' order by expire_at"
]


def set_hook(key: str, value: str, ttl_type: str | None, ttl_value: int | None) -> tuple:
    ttl_value_default = ttl_value or 100
    try:
        value_json = json.loads(value)
        if value_json.get("link").startswith("https://space.bilibili.com/"):
            if value_json.get("item"):
                return key, value, ttl_type, random.randint(ttl_value_default // 10 * 6, ttl_value_default)
            else:
                return key, value, ttl_type, ttl_value_default // 100
    except json.JSONDecodeError:
        pass

    return key, value, ttl_type, ttl_value


def from_grafana_alert_item(v):
    values = v.get("values") or {}
    return f"""<entry>
        <title>{v["status"].upper()}: {v["labels"]["alertname"]} - {v["labels"].get("filter_group", "Default")} </title>
        <id>{v["fingerprint"]}.{int(datetime.datetime.now(datetime.timezone.utc).timestamp())}.{v["status"]}</id>
        <updated>{v["startsAt"]}</updated>
        <summary>{"\n".join(f"{k}: {v}" for k, v in values.items())}</summary>
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

