# Browser smoke test for the LuCI pages against the docker rig (http://127.0.0.1:8081, root/root).
# pip install playwright; python tests/ui_test.py <screenshot-dir>  (uses /usr/bin/google-chrome)
# Note: the 403 / uci-get "Access denied" it reports come from the LuCI login page, not our views.
import sys
from playwright.sync_api import sync_playwright
B="http://127.0.0.1:8081"; S=(sys.argv[1] if len(sys.argv)>1 else ".")
errs=[]
with sync_playwright() as p:
    br=p.chromium.launch(executable_path="/usr/bin/google-chrome",headless=True,args=["--no-sandbox"])
    pg=br.new_page(viewport={"width":1200,"height":800})
    pg.on("console",lambda m: errs.append(("console@"+pg.url,m.text[:90])) if m.type in("error","warning") else None)
    pg.on("pageerror",lambda e: errs.append(("pageerror@"+pg.url,str(e)[:90])))
    pg.goto(B+"/cgi-bin/luci/")
    pg.fill("input[name=luci_username]","root"); pg.fill("input[name=luci_password]","root")
    pg.press("input[name=luci_password]","Enter"); pg.wait_for_load_state("load")
    # menu entry
    pg.goto(B+"/cgi-bin/luci/admin/services/aiagent/chat"); pg.wait_for_load_state("load")
    pg.wait_for_selector("textarea",timeout=15000); print("title:",pg.title())
    print("menu has AI Agent:", pg.locator("text=AI Agent").count()>0)
    pg.locator("textarea").first.fill("how is the router doing?")
    pg.get_by_role("button",name="Send").click()
    pg.wait_for_selector("text=(fake LLM)",timeout=15000)
    print("reply rendered: True")
    pg.screenshot(path=S+"/chat.png")
    pg.get_by_role("button",name="New conversation").click()
    print("transcript cleared after reset:", pg.locator("text=(fake LLM)").count()==0)
    pg.goto(B+"/cgi-bin/luci/admin/services/aiagent/settings"); pg.wait_for_load_state("load")
    pg.wait_for_selector("text=API endpoint",timeout=15000)
    for lab in ["API endpoint","Model","API key","Max tool rounds","LLM timeout","System prompt"]:
        print(f"settings field '{lab}':", pg.locator(f"text={lab}").count()>0)
    m=pg.locator("input[type=text],input[type=password]")
    print("api key input is password type:", pg.locator("input[type=password]").count()>=1)
    pg.screenshot(path=S+"/settings.png")
    br.close()
print("JS errors:",errs or "none")
