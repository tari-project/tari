import { SVGProps } from 'react'

const Chart = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='66'
    height='44'
    viewBox='0 0 66 44'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    {...props}
  >
    <rect
      x='0.810547'
      width='65.1892'
      height='44'
      rx='8'
      fill='#DBF6F1'
      fillOpacity='0.5'
    />
    <path
      d='M25.071 19.7395L10.5177 34.2929C9.88769 34.9229 10.3339 36 11.2248 36H56.9997C57.552 36 57.9997 35.5523 57.9997 35V8L47.1764 20.2664C46.8676 20.6165 46.3576 20.7062 45.9477 20.4827L38.8885 16.6322C38.5447 16.4447 38.123 16.4751 37.8097 16.7101L32.638 20.5889C32.4044 20.7641 32.106 20.8284 31.821 20.7651L25.9951 19.4704C25.6613 19.3963 25.3128 19.4978 25.071 19.7395Z'
      fill='url(#paint0_linear_657_68765)'
      fillOpacity='0.5'
    />
    <path
      d='M8.81055 36L24.6829 20.1277C25.1664 19.6442 25.8634 19.4412 26.5309 19.5895L31.372 20.6653C31.942 20.792 32.5388 20.6633 33.0059 20.3129L37.2953 17.0959C37.922 16.6259 38.7653 16.565 39.453 16.9401L45.2471 20.1005C46.0668 20.5476 47.0867 20.3681 47.7045 19.668L57.9997 8'
      stroke='#5F9C91'
      strokeLinecap='round'
    />
    <defs>
      <linearGradient
        id='paint0_linear_657_68765'
        x1='33.4051'
        y1='8'
        x2='33.4051'
        y2='36'
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#76A59D' />
        <stop offset='1' stopColor='#094E41' stopOpacity='0' />
      </linearGradient>
    </defs>
  </svg>
)

export default Chart
