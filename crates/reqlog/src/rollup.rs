pub const NETWORK_FOLD: &str = "
WITH rows AS (
  SELECT m.id AS member_id, l.ts,
         CASE WHEN pg_input_is_valid(l.ip, 'inet') THEN l.ip::inet END AS ip,
         l.country, l.user_agent,
         coalesce(d.headers ? 'cf-worker', false) AS proxied
  FROM api_request_log l
  JOIN members m ON m.id = coalesce(
    l.member_id,
    CASE WHEN l.key_kind = 'session'
      THEN (SELECT s.id FROM members s WHERE s.discord_id = l.actor_id) END)
  LEFT JOIN api_request_detail d ON d.ts = l.ts AND d.request_id = l.request_id
  WHERE l.ts >= $1 AND l.ts < $2 AND ($3::uuid[] IS NULL OR l.request_id = ANY($3))
),
ips AS (
  INSERT INTO api_key_ips AS t (member_id, ip_address, first_seen, last_seen, requests, country, proxied)
  SELECT member_id, ip, min(ts), max(ts), count(*),
         (array_agg(country ORDER BY ts DESC) FILTER (WHERE country IS NOT NULL))[1],
         bool_or(proxied)
  FROM rows WHERE ip IS NOT NULL
  GROUP BY member_id, ip
  ON CONFLICT (member_id, ip_address) DO UPDATE SET
    first_seen = least(t.first_seen, excluded.first_seen),
    last_seen = greatest(t.last_seen, excluded.last_seen),
    requests = t.requests + excluded.requests,
    country = coalesce(excluded.country, t.country),
    proxied = t.proxied OR excluded.proxied
)
INSERT INTO api_member_agents AS t (member_id, user_agent, first_seen, last_seen, requests)
SELECT member_id, user_agent, min(ts), max(ts), count(*)
FROM rows WHERE user_agent IS NOT NULL
GROUP BY member_id, user_agent
ON CONFLICT ON CONSTRAINT api_member_agents_key DO UPDATE SET
  first_seen = least(t.first_seen, excluded.first_seen),
  last_seen = greatest(t.last_seen, excluded.last_seen),
  requests = t.requests + excluded.requests
";
