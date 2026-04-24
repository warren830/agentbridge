// Golden Path E2E Test: Send message from Web → Agent responds → Web shows response
//
// This tests the FULL data flow, not just UI rendering:
// 1. Login
// 2. Select a session
// 3. Send a message
// 4. Verify agent response appears in the chat panel
//
// This is the functional verification that was missing before.

import { chromium } from 'playwright'

const BASE_URL = 'http://localhost:9900'
const TOKEN = 'test123'

async function main() {
  const browser = await chromium.launch({ headless: true })
  const page = await browser.newPage()

  // Collect console logs for debugging
  const logs: string[] = []
  page.on('console', m => {
    logs.push(`[${m.type()}] ${m.text()}`)
  })

  // Step 1: Login
  console.log('=== Step 1: Login ===')
  await page.goto(BASE_URL)
  await page.waitForTimeout(2000)
  await page.fill('input[type="password"]', TOKEN)
  await page.click('button[type="submit"]')
  await page.waitForTimeout(3000)

  const url = page.url()
  if (url.includes('login')) {
    console.error('FAIL: Still on login page')
    await browser.close()
    process.exit(1)
  }
  console.log('  PASS: Logged in')

  // Step 2: Verify sessions are visible
  console.log('\n=== Step 2: Sessions visible ===')
  const sessionItems = await page.$$('.session-item')
  console.log(`  Found ${sessionItems.length} sessions`)
  if (sessionItems.length === 0) {
    console.error('FAIL: No sessions visible')
    await page.screenshot({ path: '/tmp/golden-fail-no-sessions.png' })
    await browser.close()
    process.exit(1)
  }
  console.log('  PASS: Sessions visible')

  // Step 3: Click first session
  console.log('\n=== Step 3: Select session ===')
  await sessionItems[0].click()
  await page.waitForTimeout(1000)

  const chatHeader = await page.textContent('.chat-header')
  console.log(`  Chat header: ${chatHeader?.substring(0, 80)}`)
  if (!chatHeader) {
    console.error('FAIL: No chat header after selecting session')
    await page.screenshot({ path: '/tmp/golden-fail-no-header.png' })
    await browser.close()
    process.exit(1)
  }
  console.log('  PASS: Session selected, chat panel active')

  // Step 4: Send a message
  console.log('\n=== Step 4: Send message ===')
  const testMsg = `test from golden path E2E ${Date.now()}`
  await page.fill('.input-bar input', testMsg)
  await page.click('.input-bar button[type="submit"]')
  await page.waitForTimeout(1000)

  // Verify user message appears
  const messages = await page.$$('.message')
  const lastMsg = messages.length > 0 ? await messages[messages.length - 1].textContent() : ''
  console.log(`  Last message: ${lastMsg?.substring(0, 80)}`)
  if (!lastMsg?.includes('test from golden path')) {
    console.error('FAIL: User message not displayed in chat')
    await page.screenshot({ path: '/tmp/golden-fail-no-user-msg.png' })
    await browser.close()
    process.exit(1)
  }
  console.log('  PASS: User message displayed')

  // Step 5: Wait for agent response (up to 30 seconds)
  console.log('\n=== Step 5: Wait for agent response ===')
  let agentResponded = false
  for (let i = 0; i < 30; i++) {
    await page.waitForTimeout(1000)
    const allMsgs = await page.$$('.message')
    // Look for any message that's not from user (assistant, tool, system)
    for (const msg of allMsgs) {
      const cls = await msg.getAttribute('class')
      if (cls?.includes('assistant') || cls?.includes('tool') || cls?.includes('system')) {
        const text = await msg.textContent()
        if (text && !text.includes('Select a session') && text.length > 0) {
          console.log(`  Agent response (${i+1}s): ${text?.substring(0, 100)}`)
          agentResponded = true
          break
        }
      }
    }
    if (agentResponded) break
    if (i % 5 === 4) console.log(`  Waiting... ${i+1}s`)
  }

  if (!agentResponded) {
    console.error('FAIL: No agent response after 30 seconds')
    await page.screenshot({ path: '/tmp/golden-fail-no-response.png' })
    // Print console logs for debugging
    console.log('\n  Browser console logs:')
    for (const log of logs.slice(-20)) {
      console.log(`    ${log}`)
    }
    await browser.close()
    process.exit(1)
  }
  console.log('  PASS: Agent responded!')

  // Step 6: Screenshot final state
  console.log('\n=== Step 6: Final screenshot ===')
  await page.screenshot({ path: '/tmp/golden-path-success.png', fullPage: true })
  console.log('  Saved: /tmp/golden-path-success.png')

  console.log('\n=== GOLDEN PATH TEST PASSED ===')
  console.log('Verified: Login → Select session → Send message → Agent responds → Response displayed')

  await browser.close()
}

main().catch(e => {
  console.error('Golden path test FAILED:', e)
  process.exit(1)
})
