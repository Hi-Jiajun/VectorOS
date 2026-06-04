import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
      '/ws': {
        target: 'http://localhost:8080',
        ws: true
      }
    }
  },
  build: {
    // Enable minification (default for production)
    minify: 'esbuild',
    // Generate source maps only for production debugging (disabled for smaller builds)
    sourcemap: false,
    // Target modern browsers for smaller output
    target: 'es2020',
    // Optimize chunk splitting
    rollupOptions: {
      output: {
        manualChunks: {
          // Keep Svelte framework code in a separate chunk
          'svelte': ['svelte']
        }
      }
    },
    // Enable CSS code splitting
    cssCodeSplit: true,
    // Report compressed sizes
    reportCompressedSize: true,
    // Set chunk size warning limit (in KB)
    chunkSizeWarningLimit: 500,
  }
});
