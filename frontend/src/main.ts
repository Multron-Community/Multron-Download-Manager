import { mount } from 'svelte'
import './style.css'
import App from './App.svelte'
import NoticeApp from './components/NoticeApp.svelte'

const app = mount(location.hash === '#notice' ? NoticeApp : App, {
  target: document.getElementById('app')!,
})

export default app
