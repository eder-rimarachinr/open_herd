import { useEffect, useState } from "react";
import { api, Site } from "../api/client";
import styles from "./Page.module.css";

export default function SSL() {
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.sites.list()
      .then(setSites)
      .finally(() => setLoading(false));
  }, []);

  const sslSites = sites.filter((s) => s.ssl_enabled);
  const pendingSites = sites.filter((s) => !s.ssl_enabled);

  if (loading) return <div className={styles.empty}>Loading…</div>;

  return (
    <div>
      <div className={styles.header}>
        <h1 className={styles.title}>SSL Certificates</h1>
      </div>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Active certificates</h2>
        {sslSites.length === 0 ? (
          <div className={styles.empty}>No SSL certificates issued yet.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Domain</th>
                <th>Project type</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {sslSites.map((s) => (
                <tr key={s.id}>
                  <td className={styles.bold}>{s.domain}</td>
                  <td><span className="badge badge-gray">{s.project_type}</span></td>
                  <td><span className="badge badge-green">active</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {pendingSites.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>Sites without SSL</h2>
          <p className={styles.hint}>
            Enable SSL from the Sites page for individual projects.
          </p>
          <ul className={styles.list}>
            {pendingSites.map((s) => (
              <li key={s.id} className={styles.listItem}>{s.domain}</li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
