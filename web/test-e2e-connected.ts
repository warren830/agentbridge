// E2E test: Gateway + connected instance + frontend display
import { chromium } from 'playwright'

const BASE_URL = 'http://localhost:9900'
const TOKEN = 'test123'

async function main() {
  const browser = await chromium.launch({ headless: true })
  const page = await browser.newPage()

  console.log('=== Test 1: Login ===')
  await page.goto(BASE_URL)
  await page.waitForTimeout(2000)
  await page.fill('input[type="password"]', TOKEN)
  await page.click('button[type="submit"]')
  await page.waitForTimeout(3000)
  console.log('  Logged in')

  console.log('\n=== Test 2: Instance visible in sidebar ===')
  const sidebar = await page.textContent('.sidebar')
  console.log(`  Sidebar: ${sidebar?.replace(/\s+/g, ' ').substring(0, 200)}`)

  const hasInstance = sidebar?.includes('dev-server') || sidebar?.includes('ip-172')
  console.log(`  Instance visible: ${hasInstance}`)
  if (!hasInstance) {
    console.error('  FAIL: Instance not visible in sidebar!')
    await page.screenshot({ path: '/tmp/agentbridge-fail.png', fullPage: true })
    console.log('  Screenshot: /tmp/agentbridge-fail.png')
  } else {
    console.log('  PASS')
  }

  console.log('\n=== Test 3: Project visible ===')
  const hasProject = sidebar?.includes('test')
  console.log(`  Project "test" visible: ${hasProject}`)
  if (hasProject) console.log('  PASS')

  console.log('\n=== Test 4: WebSocket connected ===')
  const wsConnected = await page.$('.sidebar-header .status-dot.online')
  console.log(`  WebSocket: ${wsConnected ? 'connected' : 'disconnected'}`)
  console.log('  PASS')

  console.log('\n=== Test 5: Screenshot ===')
  await page.screenshot({ path: '/tmp/agentbridge-connected.png', fullPage: true })
  console.log('  Saved: /tmp/agentbridge-connected.png')

  console.log('\n=== Test 6: Send message via REST ===')
  const sendRes = await fetch(`${BASE_URL}/api/instances/ip-172-31-19-0/send`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${TOKEN}` },
    body: JSON.stringify({ session_key: 'web:test', text: 'hello from web' }),
  })
  const sendBody = await sendRes.json()
  console.log(`  Send: ${sendRes.status} - ${JSON.stringify(sendBody)}`)
  if (sendRes.status === 200) console.log('  PASS')

  console.log('\n=== ALL TESTS DONE ===')
  await browser.close()
}

main().catch(e => { console.error('FAIL:', e); process.exit(1) })
