<script lang="ts">
  import '../app.css';
</script>

<div class="app">
  <nav>
    <a href="/">Dashboard</a>
    <a href="/pppoe">PPPoE</a>
    <a href="/interfaces">Interfaces</a>
    <a href="/frr">FRRouting</a>
    <a href="/ipv6">IPv6</a>
    <a href="/dhcp">DHCP</a>
    <a href="/dns">DNS</a>
    <a href="/firewall">Firewall</a>
    <a href="/flow">Flow Monitor</a>
    <a href="/qos">QoS</a>
    <a href="/logs">Logs</a>
    <a href="/settings">Settings</a>
  </nav>

  <main>
    <slot />
  </main>
</div>

<style>
  .app {
    display: flex;
    min-height: 100vh;
  }

  nav {
    width: 200px;
    background: #1a1a2e;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  nav a {
    color: #e0e0e0;
    text-decoration: none;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    transition: background 0.2s;
  }

  nav a:hover {
    background: #16213e;
  }

  main {
    flex: 1;
    padding: 2rem;
    background: #0f0f23;
    color: #e0e0e0;
  }
</style>
