# Supplier fixtures (synthetic)

Test input for the import's adapters (`src/coffee/import.rs`, `docs/green-coffee-sources.md`). The
files are **synthetic**: each is shaped after what one supplier served for one product on
2026-10-25 (Shopify's `/products/<handle>.json`, and from 2026-11-02 the theme's
`/products/<handle>.js` the import reads first, WooCommerce's
`/wp-json/wc/store/v1/products?slug=<slug>`, and for Sweet Maria's also the product page's
title, score and first specs table). They keep the structure the adapters read (keys, the
HTML elements and classes the parser matches) and short facts (title, origin, region,
process, variety, altitude, score, bag sizes, prices, SKUs); every piece of prose
(descriptions, tasting text, image alt text) is invented test text, and images point at
`example.com`. Keys the adapters never read are mostly dropped.

**When a supplier changes its site**, reproduce the new shape here synthetically and fix
the adapter. Never paste a supplier's page or copy: their text is theirs.

| File | Shaped after |
|---|---|
| `sweetmarias-8647.json`, `.html` | https://www.sweetmarias.com/products/ethiopia-dry-process-kercha-haruse-8647 |
| `sweetmarias-8647.js` | the same product's `/products/<handle>.js` (2026-11-02: prices in cents, `weight`, `available`, tags as a list) |
| `sweetmarias-soldout.js` | a sold-out Sweet Maria's coffee's `.js` (2026-11-02: one "Default Title" variant, not available) |
| `bodhileaf-honduras-copan.json` | https://bodhileafcoffee.com/products/honduras-copan-organic-green |
| `thetaridge-ethiopian-harrar.json` | https://thetaridgecoffee.com/products/ethiopian-harrar |
| `ucp-profile.json` | a Shopify store's UCP business profile, `/.well-known/ucp` (2026-11-02: `ucp.capabilities["dev.ucp.shopping.permalink"][].config.endpoint`, Sweet Maria's `/buy`; the rest cut to its keys) |
| `burman-ethiopian-aricha.json` | https://burmancoffee.com/product/green-coffee-beans/ethiopian-aricha-washed/ |
