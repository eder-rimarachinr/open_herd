import styles from "./Page.module.css";

// Phase 4: database management (detection + managed installs).
export default function Database() {
  return (
    <div>
      <div className={styles.header}>
        <h1 className={styles.title}>Database</h1>
      </div>
      <div className={styles.empty}>
        Database management is coming in Phase 4.
        <br />
        <br />
        Planned support: MariaDB, PostgreSQL, MySQL (detect installed instances
        or let phpenv manage its own). SQL Server via Docker.
      </div>
    </div>
  );
}
