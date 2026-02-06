/** @type {import('next').NextConfig} */
const nextConfig = {
  // Allow importing kalam-link from local path
  transpilePackages: ['kalam-link'],
  
  // Enable experimental features for server actions
  experimental: {
    serverActions: {
      bodySizeLimit: '2mb',
    },
  },

  // Configure webpack to handle WASM files properly
  webpack: (config, { isServer }) => {
    // Add rule for .wasm files
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
    };
    
    // Configure WASM module handling
    config.module.rules.push({
      test: /\.wasm$/,
      type: 'asset/resource',
      generator: {
        filename: 'static/wasm/[name].[hash][ext]',
      },
    });

    // Ensure kalam-link WASM files are treated as external in the build
    config.resolve.fallback = {
      ...config.resolve.fallback,
      fs: false,
      path: false,
      crypto: false,
    };

    return config;
  },
};

module.exports = nextConfig;
