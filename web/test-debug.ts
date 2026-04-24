import { chromium } from 'playwright'

async function main() {
  const browser = await chromium.launch({ headless: true })
  const page = await browser.newPage()

  // Listen to console
  page.on('console', msg => console.log(`  [BROWSER] ${msg.text()}`))

  await page.goto('http://localhost:9900')
  await page.waitForTimeout(2000)

  // Login
  await page.fill('input[type="password"]', 'test123')
  await page.click('button[type="submit"]')
  await page.waitForTimeout(3000)

  // Check what data the store has
  const storeData = await page.evaluate(() => {
    // Access Pinia store via __pinia
    const app = (document as any).__vue_app__
    if (!app) return 'no vue app'
    // Try to get store data from the DOM
    return document.querySelector('.sidebar')?.innerHTML || 'no sidebar'
  })
  console.log('Store data:', typeof storeData === 'string' ? storeData.substring(0, 500) : storeData)

  // Also test REST directly from browser
  const apiResult = await page.evaluate(async () => {
    const token = localStorage.getItem('agentbridge_token')
    const res = await fetch('/api/instances', {
      headers: { Authorization: `Bearer ${token}` },
    })
    return { status: res.status, body: await res.json() }
  })
  console.log('API result:', JSON.stringify(apiResult, null, 2))

  await page.screenshot({ path: '/tmp/agentbridge-debug.png', fullPage: true })
  console.log('Screenshot saved')

  await browser.close()
}

main().catch(e => { console.error(e); process.exit(1) })
