/* eslint-disable prettier/prettier */
const translations = {
  actions: {
    skipChatting: 'Skip Chatting',
    skipOnboarding: 'Skip',
  },
  status: {
    processing: 'processing',
    done: 'done!',
    fail: 'fail!',
  },
  intro: {
    message1: {
      part1: 'Hi! My name is T-Bot.',
      part2:
        'It is a great pleasure and an honor to meet you! I have no memory of human faces, so if our paths have already crossed in the Aurora app, I’m glad to see you again!',
    },
    message2:
      'I\'m kind of like Gandalf, Dumbledore or Obi-Wan Kenobi. You know, the guy who makes sure the novice gets to a certain destination. Spoiler alert: in this saga the guide will survive. Regardless of whether this is your first contact with cryptocurrencies or you are advanced in it, I will stay with you until the Tari Launchpad setup process is successfully completed.',
    message3:
      'So let\'s get started! 🚀 The setup process usually takes 5 to 10 minutes. A duo like you and me should be able to deal with it quickly, right?',
    message4:
      'But first things first! ☕️ A warm drink will surely make the time spent together more pleasant. Unfortunately, I have no connection with your kettle or coffee machine - you have to deal with this side task yourself.',
  },
  dockerInstall: {
    message1: {
      part1: 'Ay, caramba! No more chit-chatting about coffee and tea!',
      part2: 'We need to install and run Docker',
      part3: 'on your computer because I see you don\'t have one yet.',
    },
    message2: 'Docker is an open-source platform that allows you to easily install complex software configurations (like Tari Launchpad). You can use it on Linux, Windows 10, and macOS. It acts like a lightweight virtual machine that is sandboxed from your operating system.',
    message3: {
      part1: 'Ok, I have already found the latest version of Docker compatible with the',
      part2: 'operating system you are using.',
      part3: 'Download the installation file, follow the instructions and run the Docker. ',
      part4: 'When you are done, please come back to me so that we can continue the setup process. As a responsible bot, I promise to wait for you here.',
    },
    message4: {
      part1: '🆘 I almost forgot! If you have any problems or doubts',
      part2: 'contact our awsome Tari community on Discord.',
    },
    message5: {
      link: 'Take me to installation page',
    },
    afterInstall: 'Easy peasy, lemon squeezy! Congratulations! 👏 Docker is installed on your computer. I knew you could handle it. Now it\'s time for the final stretch.',
    startSyncBtn: 'Sync the blockchain', 
  },
  dockerImages: {
    message1: {
      part1: 'I need a few minutes for further configuration. If you want to know exactly what I am doing now, open ',
      part2: 'Expert view.'
    },
    errors: {
      noSpace: 'Yikes! It looks like there is not enough space on your hard drive. It\'s time to get rid of your old vacation photos so you can start digging Tari. Let\'s be honest, the most important ones are already on your Instagram. When you are done deleting the data, hit the button below to try to download the necessary elements again.',
      serverError: {
        part1: 'Houston, we have a problem!',
        part2: 'It looks like a server error',
        part3: 'has occurred.',
        part4: 'Don’t worry – it is very possible that this is a one-off situation. Trying again will allow you to pull the images successfully. If the problem persists, please contact Tari community on Discord.',
      }
    },
  },
  lastSteps: {
    message1: 'Awesome, everything went smoothly. You are one step away from starting mining.',
    message2: 'When the data synchronization is completed, the Tari Launchpad dashboard will start automatically. Do not close this window as this will pause the entire process.',
    syncError: 'Oops! Something went wrong!',
    blockchainIsSyncing: 'Blockchain is synchronizing...',
  },
  expertView: {
    title: 'Pulling images',
  },
}

export default translations
